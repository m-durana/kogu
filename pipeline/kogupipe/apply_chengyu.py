"""Apply chinese-xinhua idiom etymologies (MIT) to an existing DB.

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_chengyu [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
Idempotent (INSERT OR IGNORE). Restart the service afterwards so the entry cache reloads.
"""
from __future__ import annotations

import os
import sqlite3
import sys

from .db import DB_PATH
from .ingest import chengyu


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"applying chengyu idiom etymologies to {path}")
    conn = sqlite3.connect(path)
    chengyu.ingest(conn)
    conn.commit()
    n = conn.execute("SELECT count(*) FROM etymology WHERE source='xinhua'").fetchone()[0]
    conn.close()
    print(f"  xinhua-sourced etymology rows now: {n}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
