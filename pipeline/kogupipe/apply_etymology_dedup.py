"""Remove homograph-bled etymologies from an existing DB.

The Wiktionary etymology ingest attaches an etymon to every lexeme sharing a surface form, so a
katakana homograph inherits a sibling's origin: サイン "signature" picks up "From English sine",
ムース "mousse" picks up "moose", ＲＡＭ "memory" picks up "lamb". This deletes an English-borrowing
etymology from a lexeme when the source word is NOT in that lexeme's own gloss but IS the whole-word
meaning of a same-reading sibling (which genuinely owns it). High precision: a real borrowing keeps
its etymology because its own gloss contains the source word (ＦＡＸ "fax" ← "fax").

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_etymology_dedup [db_path]
Idempotent. Restart the service afterwards so the entry cache reloads.
"""
from __future__ import annotations

import os
import re
import sqlite3
import sys
from collections import defaultdict

from .db import DB_PATH

ENG = re.compile(r"\b(?:from|borrowed from|abbreviation of)\s+english\s+([a-z]{3,})", re.I)


def _owns(gloss: str, x: str) -> bool:
    return re.search(r"\b" + re.escape(x) + r"\b", gloss) is not None


def find_bled(conn) -> list[int]:
    gloss: dict[int, str] = {}
    for lid, g in conn.execute(
        "SELECT lexeme_id, group_concat(lower(gloss_en),' ') FROM sense GROUP BY lexeme_id"
    ):
        gloss[lid] = g or ""
    rows = conn.execute(
        "SELECT e.lexeme_id, l.reading, e.text FROM etymology e JOIN lexeme l ON l.id=e.lexeme_id "
        "WHERE l.variety='ja' AND e.source='wiktionary'"
    ).fetchall()
    by_reading: dict[str, list[int]] = defaultdict(list)
    for lid, reading, _ in rows:
        by_reading[reading].append(lid)
    bled: list[int] = []
    for lid, reading, text in rows:
        m = ENG.search(text)
        if not m:
            continue
        x = m.group(1).lower()
        if _owns(gloss.get(lid, ""), x):
            continue  # this lexeme's own meaning IS the source word: a real borrowing, keep it
        if any(s != lid and _owns(gloss.get(s, ""), x) for s in by_reading[reading]):
            bled.append(lid)  # a same-reading sibling owns the word: this one inherited it
    return bled


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"de-bleeding homograph etymologies in {path}")
    conn = sqlite3.connect(path)
    bled = find_bled(conn)
    conn.executemany("DELETE FROM etymology WHERE lexeme_id=? AND source='wiktionary'", [(l,) for l in bled])
    conn.commit()
    conn.close()
    print(f"  removed {len(bled)} bled etymology rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
