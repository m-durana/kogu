"""Apply Wiktionary cross-language bridges to an existing DB without a full rebuild (idempotent).

Resolves the compact translation triples (sources/wiktionary_translations.tsv) into
lexeme_equivalent edges with source='wiktionary' (see ingest/translations.py). Only the
'wiktionary' rows are cleared+rebuilt, so the inline/curated edges are left intact. This touches
no senses, so no FTS rebuild is needed; restart the service so the entry handler reloads edges.

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_translations [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
"""
from __future__ import annotations

import os
import sqlite3
import sys

from .db import DB_PATH
from .ingest.translations import ingest


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"applying Wiktionary cross-language bridges to {path}")
    conn = sqlite3.connect(path)
    ingest(conn)
    conn.commit()
    n = conn.execute("SELECT COUNT(*) FROM lexeme_equivalent").fetchone()[0]
    by_src = dict(conn.execute("SELECT source, COUNT(*) FROM lexeme_equivalent GROUP BY source").fetchall())
    conn.close()
    print(f"  lexeme_equivalent rows: {n}  by source: {by_src}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
