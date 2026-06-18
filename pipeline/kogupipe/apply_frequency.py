"""Re-score lexeme.freq on an existing DB using the current frequency ingest (wordfreq-backed).

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_frequency [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
Restart the service afterwards so the search graph reloads the new scores.
"""
from __future__ import annotations

import os
import sqlite3
import sys

from .db import DB_PATH
from .ingest import frequency


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"re-scoring frequency in {path}")
    conn = sqlite3.connect(path)
    # clear stale scores first so a now-unmatched lexeme doesn't keep an old value
    conn.execute("UPDATE lexeme SET freq=NULL")
    frequency.ingest(conn)
    conn.commit()
    cov = dict(
        conn.execute(
            "SELECT variety, "
            "round(100.0*sum(CASE WHEN freq IS NOT NULL THEN 1 ELSE 0 END)/count(*),1) "
            "FROM lexeme GROUP BY variety"
        ).fetchall()
    )
    conn.close()
    print(f"  coverage %: {cov}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
