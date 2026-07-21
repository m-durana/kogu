"""Apply mapull idiom etymologies (MIT) to an existing DB.
Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_mapull [db_path]
Idempotent (INSERT OR IGNORE). Restart the service afterwards so the entry cache reloads.
"""
from __future__ import annotations
import os, sqlite3, sys
from .db import DB_PATH
from .ingest import mapull

def main(argv):
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"applying mapull idiom etymologies to {path}")
    conn = sqlite3.connect(path); mapull.ingest(conn); conn.commit()
    n = conn.execute("SELECT count(*) FROM etymology WHERE source='mapull'").fetchone()[0]
    conn.close(); print(f"  mapull-sourced etymology rows now: {n}"); return 0

if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
