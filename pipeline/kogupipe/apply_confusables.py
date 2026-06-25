"""Apply Unihan kSpoofingVariant confusables to an existing DB (creates the table if missing).

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_confusables [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
Idempotent. Restart the service afterwards so the character cache reloads.
"""
from __future__ import annotations

import os
import sqlite3
import sys

from .db import DB_PATH
from .ingest import confusables

_DDL = """
CREATE TABLE IF NOT EXISTS char_confusable (
    cp             INTEGER NOT NULL REFERENCES character(cp),
    confusable_cp  INTEGER NOT NULL REFERENCES character(cp),
    PRIMARY KEY (cp, confusable_cp)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_char_confusable_cp ON char_confusable(cp);
"""


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"applying confusables to {path}")
    conn = sqlite3.connect(path)
    conn.executescript(_DDL)
    confusables.ingest(conn)
    conn.commit()
    n = conn.execute("SELECT count(*) FROM char_confusable").fetchone()[0]
    conn.close()
    print(f"  char_confusable rows now: {n}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
