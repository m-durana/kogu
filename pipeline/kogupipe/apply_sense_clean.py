"""Drop non-meaning sense rows and stray etymology fragments from an existing DB.

Three cleanups, all idempotent:
  1. delete pure CC-CEDICT classifier senses ("CL:個", "CL:次,個") - a grammatical annotation, not a
     meaning - but only when the lexeme still has a real sense left, so nothing is orphaned.
  2. de-duplicate identical senses on the same lexeme (毘 listed "to adjoin" twice).
  3. strip orphaned "Unknown." lines that leaked from a Wiktionary section merge; delete the
     etymology row if that empties it.
Then rebuild gloss_fts so search matches the cleaned senses.

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_sense_clean [db_path]
Restart the service afterwards so the entry cache reloads.
"""
from __future__ import annotations

import os
import re
import sqlite3
import sys

from .db import DB_PATH

_UNKNOWN_LINE = re.compile(r"(?m)^[ \t]*Unknown\.[ \t]*$\n?")


def ingest(conn: sqlite3.Connection) -> tuple[int, int, int]:
    # 1. pure classifier senses, only where a real sense remains
    cl = conn.execute(
        "DELETE FROM sense WHERE gloss_en LIKE 'CL:%' AND lexeme_id IN ("
        "  SELECT lexeme_id FROM sense WHERE gloss_en NOT LIKE 'CL:%' AND gloss_en IS NOT NULL)"
    ).rowcount
    # 2. identical duplicate senses on one lexeme (keep the earliest row)
    dup = conn.execute(
        "DELETE FROM sense WHERE id NOT IN (SELECT MIN(id) FROM sense GROUP BY lexeme_id, gloss_en)"
    ).rowcount
    # 3. stray "Unknown." lines in etymology text; drop the row if nothing meaningful is left
    ety = 0
    for lid, text in conn.execute(
        "SELECT lexeme_id, text FROM etymology WHERE text LIKE '%Unknown.%'"
    ).fetchall():
        cleaned = _UNKNOWN_LINE.sub("", text).strip()
        if cleaned == text:
            continue
        if cleaned:
            conn.execute("UPDATE etymology SET text=? WHERE lexeme_id=?", (cleaned, lid))
        else:
            conn.execute("DELETE FROM etymology WHERE lexeme_id=?", (lid,))
        ety += 1
    return cl, dup, ety


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"cleaning artifact senses + etymology fragments in {path}")
    conn = sqlite3.connect(path)
    cl, dup, ety = ingest(conn)
    if cl or dup:
        conn.execute("INSERT INTO gloss_fts(gloss_fts) VALUES ('rebuild')")
        conn.execute("ANALYZE")
    conn.commit()
    conn.close()
    print(f"  removed {cl} CL: senses, {dup} duplicate senses, cleaned {ety} etymology rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
