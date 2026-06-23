"""Add / refresh Japanese pitch-accent (Kanjium accents.txt) on an existing DB.

Populates lexeme_reading.accent for ja kind='kana' rows from pipeline/sources/accents.txt
(Kanjium, CC BY-SA 4.0). Adds the nullable column if missing, so it is non-breaking for a running
backend (which selects explicit columns). Idempotent: clears prior ja-kana accents and repopulates,
guarded by build_meta('kanjium_accent'), so re-running reproduces the same state.

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_accents [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
Restart the service afterwards so the reading data reloads.
"""
from __future__ import annotations

import os
import sys

from .db import DB_PATH, connect
from .ingest import accents


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"loading Kanjium pitch accents into {path}")
    conn = connect(path)
    accents.ingest(conn)
    conn.commit()
    total = conn.execute(
        "SELECT count(*) FROM lexeme_reading lr JOIN lexeme l ON l.id=lr.lexeme_id "
        "WHERE l.variety='ja' AND lr.kind='kana' AND lr.accent IS NOT NULL"
    ).fetchone()[0]
    conn.close()
    print(f"  ja kana readings with accent={total}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
