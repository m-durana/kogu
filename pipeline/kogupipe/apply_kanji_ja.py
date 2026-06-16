"""Populate character.gloss_ja (Kanjidic Japanese-perspective English meanings) on an existing DB,
without a full rebuild (idempotent). Adds the column if missing. After running, restart the service.

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_kanji_ja [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
"""
from __future__ import annotations

import os
import sqlite3
import sys

from .db import DB_PATH
from .ingest.backbone import _kanjidic_meanings


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"applying Kanjidic Japanese meanings to {path}")
    conn = sqlite3.connect(path)
    cols = {r[1] for r in conn.execute("PRAGMA table_info(character)")}
    if "gloss_ja" not in cols:
        conn.execute("ALTER TABLE character ADD COLUMN gloss_ja TEXT")
    meanings = _kanjidic_meanings()
    conn.executemany(
        "UPDATE character SET gloss_ja=?2 WHERE cp=?1",
        [(cp, m) for cp, m in meanings.items()],
    )
    conn.commit()
    n = conn.execute("SELECT COUNT(*) FROM character WHERE gloss_ja IS NOT NULL").fetchone()[0]
    conn.close()
    print(f"  characters with gloss_ja: {n}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
