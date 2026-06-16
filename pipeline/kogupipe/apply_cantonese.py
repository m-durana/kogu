"""Apply the Cantonese 粵字 retag to an existing DB without a full rebuild (idempotent).

Relabels fully-Cantonese zh lexemes to 粵 with jyutping and splits mixed homographs (see
ingest/cantonese.py::retag), then rebuilds gloss_fts (new yue senses must be searchable) + ANALYZE.
After running, restart the service so the in-memory graph/search index reloads.

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_cantonese [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
"""
from __future__ import annotations

import os
import sqlite3
import sys

from .db import DB_PATH
from .ingest.cantonese import retag


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"applying cantonese retag to {path}")
    conn = sqlite3.connect(path)
    retag(conn)
    conn.execute("INSERT INTO gloss_fts(gloss_fts) VALUES ('rebuild')")
    conn.execute("ANALYZE")
    conn.commit()
    by_variety = dict(conn.execute("SELECT variety, COUNT(*) FROM lexeme GROUP BY variety").fetchall())
    conn.close()
    print("  lexemes by variety:", by_variety)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
