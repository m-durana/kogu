"""Add / refresh Middle Chinese (廣韻 Baxter) readings on an existing DB (char_reading kind='mc').

Reads pipeline/sources/char_mc.json (regenerate it first with `node pipeline/scripts/gen_mc.mjs`).
Idempotent: clears prior kind='mc' rows and reloads, so re-running is safe.

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_mc [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
Restart the service afterwards so the reading data reloads.
"""
from __future__ import annotations

import os
import sys

from .db import DB_PATH, connect
from .ingest import middle_chinese


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"loading Middle Chinese readings into {path}")
    conn = connect(path)
    middle_chinese.ingest(conn)
    conn.commit()
    total = conn.execute("SELECT count(*) FROM char_reading WHERE kind='mc'").fetchone()[0]
    chars = conn.execute("SELECT count(DISTINCT cp) FROM char_reading WHERE kind='mc'").fetchone()[0]
    conn.close()
    print(f"  mc rows={total} across {chars} characters")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
