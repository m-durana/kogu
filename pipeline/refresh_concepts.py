"""Rebuild the gloss-pivot concept layer in place on the live DB.

concepts.ingest is a full rebuild pass that normally runs during a from-scratch build, but the live DB
has had senses added by later refreshers (Cantonese sense-split, etc.) without concepts being rebuilt,
leaving ~900 senses unlinked even though their concept exists. This drops the gloss-pivot concepts and
re-runs the ingest against the CURRENT senses (also applying the improved normalization). OMW concepts
(source='omw') are preserved/appended separately by refresh_omw.py.

Usage: KOGU_DB=data/kogu.sqlite pipeline/.venv/bin/python pipeline/refresh_concepts.py
"""
import os
import sqlite3
import sys

sys.path.insert(0, os.path.dirname(__file__))
from kogupipe.ingest import concepts  # noqa: E402

DB = os.environ.get("KOGU_DB", "data/kogu.sqlite")


def main():
    conn = sqlite3.connect(DB)
    # drop the gloss-derived concepts (pivot + content-word token); leave omw/wiktionary/curated intact
    gp = [r[0] for r in conn.execute("SELECT id FROM concept WHERE source IN ('gloss-pivot','gloss-token')")]
    conn.execute("DELETE FROM sense_concept WHERE concept_id IN (SELECT id FROM concept WHERE source IN ('gloss-pivot','gloss-token'))")
    conn.execute("DELETE FROM concept WHERE source IN ('gloss-pivot','gloss-token')")
    # concepts.ingest assigns ids starting at 1; if other concept sources already hold low ids this
    # would collide, so shift its allocation above the current max by temporarily seeding. Simplest:
    # ingest into a clean table only when gloss-pivot was the sole source (the current state).
    other = conn.execute("SELECT COUNT(*) FROM concept").fetchone()[0]
    if other:
        print(f"  note: {other} non-gloss-pivot concepts present; ingest ids may need offsetting", file=sys.stderr)
    concepts.ingest(conn)
    conn.commit()
    n = conn.execute("SELECT COUNT(*) FROM concept WHERE source='gloss-pivot'").fetchone()[0]
    links = conn.execute("SELECT COUNT(*) FROM sense_concept").fetchone()[0]
    conn.close()
    print(f"concepts refreshed: gloss-pivot concepts={n} (was {len(gp)}), total sense-links={links}")


if __name__ == "__main__":
    main()
