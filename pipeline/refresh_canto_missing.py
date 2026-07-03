"""Recover CC-Canto words dropped by the headword-collision dedupe, in place.

The Cantonese ingest skips any CC-Canto entry whose traditional form already exists as a zh
headword ("not standard Mandarin words"). That rule silently deletes Cantonese words that merely
SHARE a written form with an unrelated literary Mandarin word: 靚 leng3 "pretty" (vs jìng), 睇
tai2 "to watch" (vs dì "look askance"), 俾 bei2 "to give" (vs bǐ) ... none of which the retag can
recover because their CC-CEDICT glosses carry no "(Cantonese)" marker.

Fix: re-walk the CC-Canto file's collided entries and create the yue lexeme when NO zh lexeme of
that headword carries a jyutping reading. Shared vocabulary always has one (the companion
cccedict-canto-readings file attaches it), so a head with zero jyutping means CC-CEDICT's word is
an unrelated literary homograph and the Cantonese word is simply missing. Same-reading entries
that merely add slang senses (上車 "buy a first flat") stay deduped, as before.

Idempotent (keyed in build_meta). Run after the cantonese ingest; safe on the live DB, but the
backend caches lexemes at startup, so restart kogu.service afterwards.

Usage: KOGU_DB=data/kogu.sqlite python3 pipeline/refresh_canto_missing.py [--dry-run]
"""
from __future__ import annotations

import os
import re
import sqlite3
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__)))
from kogupipe.ingest.cantonese import _jyut_plain, _lines, CCCANTO  # noqa: E402

DB = os.environ.get("KOGU_DB", "data/kogu.sqlite")
VERSION = "2"
# Hand-curated entries for core Cantonese words no ingested source carries standalone (CC-Canto
# only has them inside compounds). Glosses per Wiktionary's Cantonese sections.
CURATED = [
    ("睇", "睇", "tai2", ["to look at; to watch; to read", "to consider; to regard as"]),
]
# Cantonese readings attached to an EXISTING zh lexeme (same word, CC-CEDICT just lacks the
# jyutping): (headword, pinyin prefix to disambiguate homographs, jyutping)
CURATED_ZH_JYUT = [
    ("靚", "lia", "leng3"),  # 靚 liàng "attractive; good-looking" is Cantonese leng3
]
STOP = {
    "the", "and", "for", "with", "that", "this", "one", "someone", "something", "person",
    "used", "usually", "very", "also", "etc", "coll", "cantonese", "mandarin", "variant",
}
WORD = re.compile(r"[a-z]+")


def tokens(text: str) -> set[str]:
    return {t for t in WORD.findall(text.lower()) if len(t) >= 3 and t not in STOP}


def main() -> None:
    dry = "--dry-run" in sys.argv
    conn = sqlite3.connect(DB)
    if not dry:
        cur = conn.execute("SELECT value FROM build_meta WHERE key='cccanto_missing'").fetchone()
        if cur and cur[0] == VERSION:
            print("already applied")
            return

    zh_heads: dict[str, set[str]] = {}
    for head, gloss in conn.execute(
        "SELECT l.headword, s.gloss_en FROM lexeme l JOIN sense s ON s.lexeme_id=l.id WHERE l.variety='zh'"
    ):
        zh_heads.setdefault(head, set()).update(tokens(gloss))
    zh_with_jyut = {
        h for (h,) in conn.execute(
            "SELECT DISTINCT l.headword FROM lexeme l JOIN lexeme_reading r ON r.lexeme_id=l.id "
            "WHERE l.variety='zh' AND r.kind='jyutping'"
        )
    }
    # key by (head, TONED jyutping): CC-Canto lists homographs as separate lines (靚 zing6
    # "young" AND leng3 "pretty") and each deserves its own lexeme; tones must stay in the key
    # or leng1/leng3 collide.
    def jkey(j):
        return (j or "").lower().replace(" ", "")
    yue_by_key: dict[tuple[str, str], int] = {}
    for lid, h, r in conn.execute(
        "SELECT l.id, l.headword, r.value FROM lexeme l "
        "LEFT JOIN lexeme_reading r ON r.lexeme_id=l.id AND r.kind='jyutping' WHERE l.variety='yue'"
    ):
        yue_by_key[(h, jkey(r))] = lid
    yue_keys = set(yue_by_key)
    yue_heads_bare = {h for h, _ in yue_keys}

    next_lex = conn.execute("SELECT COALESCE(MAX(id),0) FROM lexeme").fetchone()[0]
    next_sf = conn.execute("SELECT COALESCE(MAX(id),0) FROM surface_form").fetchone()[0]
    next_sense = conn.execute("SELECT COALESCE(MAX(id),0) FROM sense").fetchone()[0]

    for head, pin, j in CURATED_ZH_JYUT:
        for (lid,) in conn.execute(
            "SELECT id FROM lexeme WHERE variety='zh' AND headword=? AND reading LIKE ?", (head, pin + "%")
        ):
            if not dry:
                conn.execute("INSERT OR IGNORE INTO lexeme_reading(lexeme_id,kind,value) VALUES (?,?,?)", (lid, "jyutping", j))
                conn.execute(
                    "INSERT OR IGNORE INTO lexeme_reading(lexeme_id,kind,value) VALUES (?,?,?)",
                    (lid, "jyutping_plain", j.rstrip("123456")),
                )

    created = []
    entries = list(_lines(CCCANTO)) + [(t, sp, "", j, g) for t, sp, j, g in CURATED]
    for trad, simp, _pinyin, jyut, glosses in entries:
        if not glosses or trad not in zh_heads:
            continue  # only the collided-and-dropped entries
        key = (trad, jkey(jyut))
        curated = (trad, simp, jyut, glosses) in [(t, sp, j, g) for t, sp, j, g in CURATED]
        if key in yue_keys or (not jyut and trad in yue_heads_bare):
            if curated:
                # curated entry for a word that exists with a poor source gloss: append the senses
                lid = yue_by_key.get(key)
                if lid:
                    have = {g for (g,) in conn.execute("SELECT gloss_en FROM sense WHERE lexeme_id=?", (lid,))}
                    for g in glosses:
                        if g not in have and not dry:
                            next_sense += 1
                            conn.execute(
                                "INSERT INTO sense(id,lexeme_id,pos,gloss_en,sense_order) VALUES (?,?,?,?,?)",
                                (next_sense, lid, None, g, len(have)),
                            )
            continue  # this exact word already exists as yue
        if trad in zh_with_jyut:
            continue  # the zh entry IS the Cantonese word too (shared vocabulary): keep deduped
        canto_tok = set()
        for g in glosses:
            canto_tok |= tokens(g)
        if not canto_tok:
            continue  # gloss is all stopwords/cross-references: nothing usable to show
        if canto_tok & zh_heads[trad]:
            continue  # glosses share content words with the zh entry: same word, paraphrased
        created.append((trad, simp, jyut, glosses))
        if dry:
            continue
        next_lex += 1
        lid = next_lex
        conn.execute(
            "INSERT INTO lexeme(id,variety,headword,reading,freq,freq_source) VALUES (?,?,?,?,?,?)",
            (lid, "yue", trad, jyut or None, None, None),
        )
        next_sf += 1
        conn.execute(
            "INSERT INTO surface_form(id,lexeme_id,form,script,region,is_primary) VALUES (?,?,?,?,?,?)",
            (next_sf, lid, trad, "trad", "HK", 1),
        )
        if simp != trad:
            next_sf += 1
            conn.execute(
                "INSERT INTO surface_form(id,lexeme_id,form,script,region,is_primary) VALUES (?,?,?,?,?,?)",
                (next_sf, lid, simp, "simp", "CN", 0),
            )
        if jyut:
            conn.execute("INSERT OR IGNORE INTO lexeme_reading(lexeme_id,kind,value) VALUES (?,?,?)", (lid, "jyutping", jyut))
            conn.execute(
                "INSERT OR IGNORE INTO lexeme_reading(lexeme_id,kind,value) VALUES (?,?,?)",
                (lid, "jyutping_plain", _jyut_plain(jyut)),
            )
        for order, g in enumerate(glosses):
            next_sense += 1
            conn.execute(
                "INSERT INTO sense(id,lexeme_id,pos,gloss_en,sense_order) VALUES (?,?,?,?,?)",
                (next_sense, lid, None, g, order),
            )
        yue_keys.add(key)
        yue_by_key[key] = lid

    if dry:
        print(f"would create {len(created)} yue lexemes:")
        for trad, _s, jyut, gl in created:
            print(f"  {trad} {{{jyut}}} {'; '.join(gl)[:90]}")
    else:
        conn.execute("INSERT OR REPLACE INTO build_meta(key,value) VALUES ('cccanto_missing',?)", (VERSION,))
        conn.commit()
        print(f"created {len(created)} yue lexemes")


if __name__ == "__main__":
    main()
