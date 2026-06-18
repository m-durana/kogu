"""Strip CC-CEDICT `trad|simp[pinyin]` cross-reference markup from existing glosses (idempotent).

CC-CEDICT embeds references as `trad|simp[pin1yin1]` inside its English glosses. Left raw they leak
the simplified twin and a tone-numbered romanisation into the prose, and rare referents render as
tofu (〡 → "numeral 1 in the Suzhou numeral system 蘇州碼子|苏州码子[Su1 zhou1 ma3 zi5]"). This rewrites
sense.gloss_en in place using the same cleaner the full build now applies, then rebuilds gloss_fts so
search stays consistent. Re-running is a no-op (already-clean glosses don't match).

Run:  cd pipeline && .venv/bin/python -m kogupipe.apply_gloss_clean [db_path]
      (defaults to data/kogu.sqlite, or set KOGU_DB)
After running, restart the service is NOT required (no in-memory gloss cache), but harmless.
"""
from __future__ import annotations

import os
import sqlite3
import sys

from .db import DB_PATH
from .ingest.lexemes import clean_gloss


def ingest(conn: sqlite3.Connection) -> int:
    """Rewrite changed glosses; return the count actually modified."""
    rows = conn.execute("SELECT id, gloss_en FROM sense").fetchall()
    updates = []
    for sid, g in rows:
        if g is None:
            continue
        cleaned = clean_gloss(g)
        if cleaned != g:
            updates.append((cleaned, sid))
    conn.executemany("UPDATE sense SET gloss_en = ? WHERE id = ?", updates)
    return len(updates)


def main(argv: list[str]) -> int:
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"cleaning CC-CEDICT gloss markup in {path}")
    conn = sqlite3.connect(path)
    n = ingest(conn)
    if n:
        # external-content FTS must be rebuilt so the cleaned glosses are what search sees.
        conn.execute("INSERT INTO gloss_fts(gloss_fts) VALUES ('rebuild')")
        conn.execute("ANALYZE")
    conn.commit()
    conn.close()
    print(f"  rewrote {n} glosses; gloss_fts rebuilt" if n else "  nothing to clean (already done)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
