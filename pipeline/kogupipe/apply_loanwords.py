"""Apply JMdict loanword origins (lsource → etymology gap-fill + badges) to an existing DB.

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_loanwords [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
Idempotent (INSERT OR IGNORE). Restart the service afterwards so the entry cache reloads.
"""
from __future__ import annotations

import os
import sqlite3
import sys

from .db import DB_PATH
from .ingest import loanwords


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"applying loanword origins to {path}")
    conn = sqlite3.connect(path)
    loanwords.ingest(conn)
    conn.commit()
    n = conn.execute("SELECT count(*) FROM etymology WHERE source='jmdict'").fetchone()[0]
    conn.close()
    print(f"  jmdict-sourced etymology rows now: {n}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
