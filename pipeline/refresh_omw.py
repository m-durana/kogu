"""Add/refresh Open Multilingual Wordnet cross-language concepts on the live DB (idempotent)."""
import os
import sqlite3
import sys

sys.path.insert(0, os.path.dirname(__file__))
from kogupipe.ingest import omw  # noqa: E402

DB = os.environ.get("KOGU_DB", "data/kogu.sqlite")


def main():
    conn = sqlite3.connect(DB)
    # clear any prior omw concepts so re-runs don't duplicate
    conn.execute("DELETE FROM sense_concept WHERE concept_id IN (SELECT id FROM concept WHERE source='omw')")
    conn.execute("DELETE FROM concept WHERE source='omw'")
    omw.ingest(conn)
    conn.commit()
    n = conn.execute("SELECT COUNT(*) FROM concept WHERE source='omw'").fetchone()[0]
    conn.close()
    print(f"omw refreshed: omw concepts={n}")


if __name__ == "__main__":
    main()
