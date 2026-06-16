"""Apply explicit equivalence edges to an existing DB without a full rebuild (idempotent).

Parses CC-Canto inline "Mandarin equivalent" notes and loads the curated equivalence maps into
lexeme_equivalent (see ingest/equivalents.py). No FTS rebuild needed - this touches no senses.
After running, restart the service so the entry handler picks up the new edges.

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_equivalents [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
"""
from __future__ import annotations

import os
import sqlite3
import sys

from .db import DB_PATH
from .ingest.equivalents import ingest


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"applying equivalence edges to {path}")
    conn = sqlite3.connect(path)
    ingest(conn)
    conn.commit()
    n = conn.execute("SELECT COUNT(*) FROM lexeme_equivalent").fetchone()[0]
    by_rel = dict(conn.execute("SELECT relation, COUNT(*) FROM lexeme_equivalent GROUP BY relation").fetchall())
    conn.close()
    print(f"  lexeme_equivalent rows: {n}  by relation: {by_rel}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
