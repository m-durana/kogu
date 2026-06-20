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

# omw lexicon → which Kogu varieties its lemmas may match
_LEXICONS = [("omw-ja:1.4", {"ja"}), ("omw-cmn:1.4", {"zh", "yue"})]
# the cross-language synset key: the trailing "NNNNNNNN-p" offset (same across languages)
_OFFSET = re.compile(r"(\d{8}-[nvars])$")
# lemma cleanup: omw uses "+" as a morpheme boundary and "～" as a placeholder
_CLEAN = re.compile(r"[+～~\s]+")
# OMW concept ids live in their own high range so they never collide with gloss-pivot ids
_ID_BASE = 1_000_000

_WN_CLASS = {"n": "n", "v": "v", "a": "a", "s": "a", "r": "r"}


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
        print("      omw: `wn` not installed — skipping (run: pip install wn && wn.download omw-ja/omw-cmn)")
        return

    # form → [(lexeme_id, variety, sense0_id, sense0_pos, gloss)] from EVERY surface form (trad+simp+kana)
    form_map: dict[str, list[tuple[int, str, int, str | None, str | None]]] = defaultdict(list)
    rows = conn.execute(
        "SELECT sf.form, l.id, l.variety, s.id, s.pos, s.gloss_en "
        "FROM surface_form sf JOIN lexeme l ON l.id = sf.lexeme_id "
        "JOIN sense s ON s.lexeme_id = l.id AND s.sense_order = 0"
    ).fetchall()
    for form, lid, var, sid, pos, gloss in rows:
        form_map[form].append((lid, var, sid, pos, gloss))

    # synset offset → {lexeme_id: sense0_id}, plus the set of varieties seen and a readable label
    matched: dict[str, dict[int, int]] = defaultdict(dict)
    varset: dict[str, set[str]] = defaultdict(set)
    label_of: dict[str, str] = {}

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
                for lid, var, sid, spos, gloss in form_map.get(form, ()):
                    if var not in varieties:
                        continue
                    # POS compatibility (only when the sense carries a JMdict pos): skip a clear mismatch
                    if spos and wc:
                        jc = _jmdict_classes(spos)
                        if jc and wc not in jc:
                            continue
                    matched[off][lid] = sid
                    varset[off].add(var)
                    if off not in label_of and gloss:
                        # a readable English label from a member's first gloss segment (≤48 chars)
                        seg = re.split(r"[;/]", gloss)[0].strip().strip(".,;:")
                        if seg:
                            label_of[off] = seg[:48]

    concept_rows = []
    link_rows = []
    cid = _ID_BASE
    for off, lexmap in matched.items():
        if len(lexmap) < 2 or len(varset[off]) < 2:
            continue  # a bridge needs ≥2 lexemes spanning ≥2 languages
        cid += 1
        concept_rows.append((cid, label_of.get(off, off), None, "omw", len(lexmap)))
        for sid in lexmap.values():
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
