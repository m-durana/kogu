"""Cross-language linking via Open Multilingual Wordnet synsets (the "proper" concept layer).

The gloss-pivot (concepts.py) links senses whose English gloss matches; this adds links by shared
WordNet SYNSET, so equivalents bridge even when their English wording differs (貓/猫 cat, 駅/站 where
COW has it). Japanese Wordnet (omw-ja) and Chinese Open Wordnet (omw-cmn) attach native CJK lemmas to
synsets that carry a stable cross-language offset id; a synset matched in ≥2 of our varieties is a
bridge. We reuse the existing concept/sense_concept tables (concept.source='omw', confidence 0.7 since
the link is lemma-level, not gloss-level). Cantonese has no wordnet, so yue only benefits where a
Chinese lemma happens to share its Han spelling.

Requires the `wn` package with omw-ja:1.4 and omw-cmn:1.4 downloaded (wn.download).
"""
from __future__ import annotations

import re
from collections import defaultdict

try:
    import wn
except ImportError:
    wn = None

from ..db import SOURCES_DIR

# omw lexicon → which Kogu varieties its lemmas may match, and the vendored LMF for offline builds
_LEXICONS = [("omw-ja:1.4", {"ja"}), ("omw-cmn:1.4", {"zh", "yue"})]
_VENDORED = {"omw-ja:1.4": "wn/omw-ja.xml.xz", "omw-cmn:1.4": "wn/omw-cmn.xml.xz"}


def _ensure_loaded() -> None:
    """Load each lexicon from the vendored LMF if it isn't already in the wn cache (offline-capable)."""
    have = {f"{lx.id}:{lx.version}" for lx in wn.lexicons()}
    for spec, rel in _VENDORED.items():
        if spec in have:
            continue
        path = SOURCES_DIR / rel
        if path.exists():
            wn.add(str(path), progress_handler=None)
# the cross-language synset key: the trailing "NNNNNNNN-p" offset (same across languages)
_OFFSET = re.compile(r"(\d{8}-[nvars])$")
# lemma cleanup: omw uses "+" as a morpheme boundary and "～" as a placeholder
_CLEAN = re.compile(r"[+～~\s]+")
# OMW concept ids live in their own high range so they never collide with gloss-pivot ids
_ID_BASE = 1_000_000

_WN_CLASS = {"n": "n", "v": "v", "a": "a", "s": "a", "r": "r"}

# content words too generic to confirm a shared meaning
_STOP = {"the", "and", "for", "with", "used", "etc", "sth", "someone", "something", "one", "that",
         "this", "form", "kind", "type", "thing", "person", "make", "way", "part", "from", "into"}


def _words(gloss: str | None) -> frozenset[str]:
    """Content words of a gloss's FIRST sense segment (before ';'), for meaning-overlap checks."""
    if not gloss:
        return frozenset()
    seg = gloss.split(";")[0]
    return frozenset(
        w for w in re.split(r"[^a-z]+", seg.lower()) if len(w) >= 3 and w not in _STOP
    )


def _jmdict_classes(pos_str: str | None) -> set[str]:
    """Coarse POS classes from a JMdict pos string ('adj-na', 'v5aru,vi', 'n,adv') → {n,v,a,r}."""
    cls: set[str] = set()
    for t in (pos_str or "").split(","):
        t = t.strip()
        if not t:
            continue
        if t.startswith("n") or t == "pn":
            cls.add("n")
        elif t.startswith("v"):
            cls.add("v")
        elif t.startswith("adj"):
            cls.add("a")
        elif t.startswith("adv"):
            cls.add("r")
    return cls


def ingest(conn) -> None:
    if wn is None:
        print("      omw: `wn` not installed — skipping (pip install -r pipeline/requirements.txt)")
        return
    _ensure_loaded()

    # form → [(lexeme_id, variety, sense0_id, sense0_pos, sense0_words)] from EVERY surface form
    # (trad+simp+kana). sense0_words are the content words of the PRIMARY gloss — used to verify a
    # synset bridge actually shares meaning at sense 0 (the handler only bridges sense_order=0).
    form_map: dict[str, list[tuple[int, str, int, str | None, frozenset[str]]]] = defaultdict(list)
    rows = conn.execute(
        "SELECT sf.form, l.id, l.variety, s.id, s.pos, s.gloss_en "
        "FROM surface_form sf JOIN lexeme l ON l.id = sf.lexeme_id "
        "JOIN sense s ON s.lexeme_id = l.id AND s.sense_order = 0"
    ).fetchall()
    label_map: dict[int, str] = {}
    for form, lid, var, sid, pos, gloss in rows:
        form_map[form].append((lid, var, sid, pos, _words(gloss)))
        if sid not in label_map and gloss:
            seg = re.split(r"[;/]", gloss)[0].strip().strip(".,;:")
            if seg:
                label_map[sid] = seg[:48]

    # synset offset → {lexeme_id: (variety, sense0_id, sense0_words)}
    matched: dict[str, dict[int, tuple[str, int, frozenset[str]]]] = defaultdict(dict)

    for lexname, varieties in _LEXICONS:
        try:
            w = wn.Wordnet(lexname)
        except Exception as e:  # noqa: BLE001
            print(f"      omw: cannot load {lexname}: {e}")
            continue
        for ss in w.synsets():
            m = _OFFSET.search(ss.id)
            if not m:
                continue
            off = m.group(1)
            wc = _WN_CLASS.get(ss.pos)
            for lemma in ss.lemmas():
                form = _CLEAN.sub("", lemma)
                if not form:
                    continue
                for lid, var, sid, spos, words in form_map.get(form, ()):
                    if var not in varieties:
                        continue
                    if spos and wc:  # POS mismatch guard (only when the sense carries a JMdict pos)
                        jc = _jmdict_classes(spos)
                        if jc and wc not in jc:
                            continue
                    matched[off][lid] = (var, sid, words)

    concept_rows = []
    link_rows = []
    cid = _ID_BASE
    for off, lexmap in matched.items():
        if len(lexmap) < 2:
            continue
        # A synset attaches to a whole lemma, not a sense — so 站 (station+stand) matched the "stand"
        # synset and would wrongly bridge its STATION sense to 起きる. Guard against that: keep a
        # lexeme only if its PRIMARY gloss shares a content word with another matched lexeme's primary
        # gloss (i.e. their sense-0 meanings genuinely overlap). A word shared by ≥2 lexemes is the
        # consensus meaning of the synset for our data; lexemes whose sense 0 isn't about that are dropped.
        wordcount: dict[str, int] = defaultdict(int)
        for _var, _sid, words in lexmap.values():
            for wd in words:
                wordcount[wd] += 1
        shared = {wd for wd, n in wordcount.items() if n >= 2}
        if not shared:
            continue
        kept = {lid: v for lid, v in lexmap.items() if v[2] & shared}
        if len({v[0] for v in kept.values()}) < 2:
            continue  # still need ≥2 languages after the meaning check
        cid += 1
        first_sid = next(iter(kept.values()))[1]
        concept_rows.append((cid, label_map.get(first_sid, off), None, "omw", len(kept)))
        for _v, sid, _w in kept.values():
            link_rows.append((sid, cid, 0.7))

    conn.executemany(
        "INSERT OR IGNORE INTO concept(id,label_en,definition,source,member_count) VALUES (?,?,?,?,?)",
        concept_rows,
    )
    conn.executemany(
        "INSERT OR IGNORE INTO sense_concept(sense_id,concept_id,confidence) VALUES (?,?,?)",
        link_rows,
    )
    print(f"      omw concepts={len(concept_rows)} links={len(link_rows)}")
