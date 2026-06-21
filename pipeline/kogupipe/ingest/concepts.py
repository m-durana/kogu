"""Phase 2.1 - concept layer via English-gloss pivot (DESIGN.md §2.3, step 3).

This first cut needs no new downloads: it clusters senses that share a normalised English gloss
into language-independent concepts. That alone powers translation search and cognate/false-friend
labelling (機場↔空港 share "airport"; 會社"guild" vs 会社"company" land in *different* concepts -
a false friend). OMW + Wiktionary translation tables are layered on later to widen coverage.
"""
from __future__ import annotations

import re
from collections import defaultdict

_PAREN = re.compile(r"[(\[{][^)\]}]*[)\]}]")
_WS = re.compile(r"\s+")

# function words that are useless as a concept key on their own
_STOP = {
    "a", "an", "the", "to", "of", "in", "on", "at", "by", "for", "and", "or", "be", "is",
    "as", "it", "that", "this", "with", "from", "etc", "esp", "e.g", "i.e", "one", "used",
}

# Upper bound on a full-segment concept's size. Concepts must still EXIST for cognate detection
# (shares_concept) even when a meaning is common — with the full JMdict, "sky"/"heaven"/"flower"
# cluster well past the old 60 and were being dropped entirely, breaking 天/本 cognate rescue and the
# everyday-word link. The handler still applies its own (smaller) member_count cap when SURFACING a
# related/synonym list, so a large concept can power detection without flooding the UI.
_MAX_LEXEMES_PER_CONCEPT = 400
# token (content-word) concepts cluster more broadly than full-segment ones, so cap them tighter.
_MAX_TOKEN_LEXEMES = 30

# generic content words that would make noisy token-concepts; excluded from the content-word pivot
# (the full-segment pivot is unaffected — only the secondary token keys skip these).
_GENERIC = {
    "thing", "things", "person", "people", "make", "made", "making", "do", "does", "have", "has",
    "go", "goes", "get", "take", "put", "way", "ways", "kind", "type", "sort", "form", "part",
    "place", "time", "work", "use", "using", "call", "name", "number", "large", "small", "big",
    "little", "good", "bad", "high", "low", "old", "new", "long", "short", "man", "woman", "child",
    "act", "action", "state", "matter", "piece", "item", "object", "area", "point", "line", "set",
    "group", "something", "someone", "somebody", "various", "certain", "particular", "general",
    "common", "way", "able", "into", "out", "off", "non", "per", "via", "etc",
}

# high-value near-synonym folding: different English wordings for the SAME concept that the exact-match
# pivot would otherwise split (駅 "train station" vs 站 "station"; 機場 "airport" vs an "airfield" gloss).
# Curated and conservative — only unambiguous equivalences, never hyponyms.
_SYNONYM = {
    "railway station": "station", "train station": "station", "railroad station": "station",
    "bus station": "station", "bus stop": "station", "railroad": "railway",
    "airfield": "airport", "aerodrome": "airport",
    "automobile": "car", "motorcar": "car", "vehicle": "car",
    "doctor": "physician", "movie": "film", "motion picture": "film",
    "spectacles": "glasses", "eyeglasses": "glasses",
    "mum": "mother", "mom": "mother", "mommy": "mother", "mama": "mother",
    "dad": "father", "papa": "father",
    "beloved": "love", "affection": "love",
}

# strip a single trailing plural -s so "dogs" and "dog" share a concept. Conservative: only a
# consonant+s ending (skips gas/bus/bias and -ss), plus the -ies→-y rule. Multi-word keys get only
# their LAST word (head noun) singularised.
_VOWELS = set("aeiou")


def _singularise(w: str) -> str:
    if len(w) > 3 and w.endswith("ies"):
        return w[:-3] + "y"
    if (
        len(w) > 3
        and w.endswith("s")
        and not w.endswith("ss")
        and w[-2] not in _VOWELS  # consonant+s (dogs, cats), not gas/bus/bias
    ):
        return w[:-1]
    return w


def normalize_gloss_segment(seg: str) -> str | None:
    """Normalise one gloss segment to a concept key, or None if unusable."""
    s = _PAREN.sub(" ", seg)            # drop parentheticals
    s = s.replace("/", " ").strip().lower()
    s = _WS.sub(" ", s)
    s = s.strip(" .,;:!?\"'")
    if s.startswith("to "):             # infinitive marker
        s = s[3:].strip()
    if s.startswith("be "):             # copular/stative ("be loyal" ↔ "loyal")
        s = s[3:].strip()
    if s.endswith(" to"):               # dative complement ("loyal to" ↔ "loyal", "belong to" ↔ "belong")
        s = s[:-3].strip()
    # drop CC-CEDICT meta markers: classifiers (CL:...), variant/see-also notations
    if s.startswith(("cl:", "see ", "variant of", "old variant", "also written")):
        return None
    if len(s) < 2 or len(s) > 48:
        return None
    if not any(c.isalpha() for c in s):
        return None
    # singularise the head (last word) so plurals fold together
    words = s.split()
    if words:
        words[-1] = _singularise(words[-1])
        s = " ".join(words)
    s = _SYNONYM.get(s, s)              # fold curated near-synonyms
    if s in _STOP:
        return None
    return s


def _keys(gloss: str):
    seen = set()
    for seg in gloss.split(";"):
        k = normalize_gloss_segment(seg)
        if k and k not in seen:
            seen.add(k)
            yield k


def _content_tokens(seg: str):
    """Content words of a gloss segment, for the secondary (token) pivot. Multi-word glosses thus also
    cluster on their content words (噤聲令 'gag order' → 'gag'), widening cross-language coverage for
    uniquely-worded entries the exact-segment pivot leaves orphaned."""
    s = _PAREN.sub(" ", seg).replace("/", " ").lower()
    s = _WS.sub(" ", s).strip(" .,;:!?\"'")
    if s.startswith(("cl:", "see ", "variant of", "old variant", "also written")):
        return
    for w in s.split():
        w = w.strip(" .,;:!?\"'-")
        if len(w) < 3 or not w.isalpha():
            continue
        w = _SYNONYM.get(_singularise(w), _singularise(w))
        if len(w) < 3 or w in _STOP or w in _GENERIC:
            continue
        yield w


def ingest(conn) -> None:
    # Two layers per key: STRONG = the key is a lexeme's full normalised gloss segment (exact meaning);
    # TOKEN = the key is just a content word of a multi-word gloss (looser). Tracked separately so a
    # common word's many TOKEN occurrences ("flower" appearing inside "flower bud", "X flower", …) can
    # never inflate — and thereby drop — the precise exact-gloss concept.
    s_lex: dict[str, set[int]] = defaultdict(set)
    s_sen: dict[str, set[int]] = defaultdict(set)
    t_lex: dict[str, set[int]] = defaultdict(set)
    t_sen: dict[str, set[int]] = defaultdict(set)

    rows = conn.execute("SELECT id, lexeme_id, gloss_en FROM sense").fetchall()
    for sense_id, lexeme_id, gloss in rows:
        if not gloss:
            continue
        seg_keys: set[str] = set()
        for seg in gloss.split(";"):
            full = normalize_gloss_segment(seg)
            if full:
                s_lex[full].add(lexeme_id)
                s_sen[full].add(sense_id)
                seg_keys.add(full)
        for seg in gloss.split(";"):
            for tok in _content_tokens(seg):
                if tok in seg_keys:
                    continue  # a single-word segment is already a strong key
                t_lex[tok].add(lexeme_id)
                t_sen[tok].add(sense_id)

    concept_rows = []
    link_rows = []
    cid = 0
    n_tok = 0
    for key in set(s_lex) | set(t_lex):
        strong_lex = s_lex.get(key, set())
        if len(strong_lex) >= 2:
            # a real exact-gloss concept. Add token members too, UNLESS that would over-inflate it —
            # then keep the precise concept (exact-gloss members only) rather than drop it entirely.
            if len(strong_lex) > _MAX_LEXEMES_PER_CONCEPT:
                continue  # the exact gloss itself is hopelessly generic
            combined = strong_lex | t_lex.get(key, set())
            if len(combined) <= _MAX_LEXEMES_PER_CONCEPT:
                senses, mc = s_sen[key] | t_sen.get(key, set()), len(combined)
            else:
                senses, mc = s_sen[key], len(strong_lex)
            source = "gloss-pivot"
        else:
            # token-only (or a single exact-gloss lexeme plus tokens): a looser content-word concept
            combined = strong_lex | t_lex.get(key, set())
            if not (2 <= len(combined) <= _MAX_TOKEN_LEXEMES):
                continue
            senses, mc = s_sen.get(key, set()) | t_sen.get(key, set()), len(combined)
            source = "gloss-token"
            n_tok += 1
        cid += 1
        concept_rows.append((cid, key, None, source, mc))
        strong_for_key = s_sen.get(key, ())
        for sense_id in senses:
            # 1.0 for an exact-gloss link (drives cognate rescue); 0.5 for a content-word link
            link_rows.append((sense_id, cid, 1.0 if sense_id in strong_for_key else 0.5))

    conn.executemany(
        "INSERT INTO concept(id,label_en,definition,source,member_count) VALUES (?,?,?,?,?)",
        concept_rows)
    conn.executemany(
        "INSERT OR IGNORE INTO sense_concept(sense_id,concept_id,confidence) VALUES (?,?,?)",
        link_rows)
    print(f"      concepts={len(concept_rows)} (token={n_tok}) sense-links={len(link_rows)}")
