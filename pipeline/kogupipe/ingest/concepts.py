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

# segments this generic create giant useless concepts; skip keys shared by more than this. Raised from
# 40: the handler now surfaces concepts up to this size (was a stricter 18), ranked by specificity, so
# slightly broader concepts (common verbs/adjectives like "change", "support") can bridge too.
_MAX_LEXEMES_PER_CONCEPT = 60

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


def ingest(conn) -> None:
    # key -> {sense_id}, key -> {lexeme_id}
    key_senses: dict[str, set[int]] = defaultdict(set)
    key_lexemes: dict[str, set[int]] = defaultdict(set)

    rows = conn.execute("SELECT id, lexeme_id, gloss_en FROM sense").fetchall()
    for sense_id, lexeme_id, gloss in rows:
        if not gloss:
            continue
        for k in _keys(gloss):
            key_senses[k].add(sense_id)
            key_lexemes[k].add(lexeme_id)

    concept_rows = []
    link_rows = []
    cid = 0
    for key, lexemes in key_lexemes.items():
        # a concept must link at least two distinct lexemes, and not be hopelessly generic
        if len(lexemes) < 2 or len(lexemes) > _MAX_LEXEMES_PER_CONCEPT:
            continue
        cid += 1
        concept_rows.append((cid, key, None, "gloss-pivot", len(lexemes)))
        for sense_id in key_senses[key]:
            link_rows.append((sense_id, cid, 1.0))

    conn.executemany(
        "INSERT INTO concept(id,label_en,definition,source,member_count) VALUES (?,?,?,?,?)",
        concept_rows)
    conn.executemany(
        "INSERT OR IGNORE INTO sense_concept(sense_id,concept_id,confidence) VALUES (?,?,?)",
        link_rows)
    print(f"      concepts={len(concept_rows)} sense-links={len(link_rows)}")
