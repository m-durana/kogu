"""Apply phono-semantic component roles to an existing DB without a full rebuild (idempotent).

Loads sources/components.jsonl (produced by extract_components.py) into char_component, keeping only
components whose character is in our backbone. No FTS rebuild needed — this touches no senses. After
running, restart the service so the entry handler picks up the roles.

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_components [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
"""
from __future__ import annotations

import json
import os
import sqlite3
import sys

from .db import DB_PATH, SOURCES_DIR

SRC = SOURCES_DIR / "components.jsonl"

_DDL = """
CREATE TABLE IF NOT EXISTS char_component (
    cp        INTEGER NOT NULL,
    ord       INTEGER NOT NULL,
    component TEXT NOT NULL,
    role      TEXT,
    gloss     TEXT,
    PRIMARY KEY (cp, ord)
) WITHOUT ROWID;
"""


def ingest(conn: sqlite3.Connection) -> int:
    conn.executescript(_DDL)
    conn.execute("DELETE FROM char_component")  # idempotent rebuild
    have = {cp for (cp,) in conn.execute("SELECT cp FROM character")}
    rows = []
    with open(SRC, encoding="utf-8") as f:
        for line in f:
            rec = json.loads(line)
            cp = ord(rec["char"])
            if cp not in have:
                continue
            for i, c in enumerate(rec["components"]):
                rows.append((cp, i, c["ch"], c.get("role"), c.get("gloss")))
    conn.executemany(
        "INSERT OR REPLACE INTO char_component(cp,ord,component,role,gloss) VALUES (?,?,?,?,?)", rows
    )
    return len(rows)


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"applying component roles to {path}")
    conn = sqlite3.connect(path)
    n = ingest(conn)
    conn.commit()
    chars = conn.execute("SELECT COUNT(DISTINCT cp) FROM char_component").fetchone()[0]
    by_role = dict(conn.execute("SELECT role, COUNT(*) FROM char_component GROUP BY role").fetchall())
    conn.close()
    print(f"  char_component rows: {n} across {chars} characters; by role: {by_role}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
