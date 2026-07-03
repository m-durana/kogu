"""Re-split CC-Canto yue senses that the old ingest collapsed onto one "; "-joined line.

The CC-Canto ingest used to store every "/"-separated sense as a single "; "-joined sense row, so
Cantonese definitions showed as one line instead of enumerating. The ingest is now fixed (one row per
"/"-sense); this migration repairs the EXISTING live DB without a full rebuild.

It only touches a yue lexeme when it currently has exactly ONE sense row whose gloss equals the
"; "-join of that headword's CC-Canto source senses AND the source has >1 sense: so CC-CEDICT-derived
(retag) yue lexemes and genuine single-sense entries are left untouched.

Usage: KOGU_DB=/path/to.sqlite pipeline/.venv/bin/python pipeline/migrate_canto_senses.py
"""
import os
import re
import sqlite3
import sys

DB = os.environ.get("KOGU_DB", "data/kogu.sqlite")
SRC = "pipeline/sources/cccanto-webdist.txt"

LINE = re.compile(r"^(\S+)\s+(\S+)\s+\[([^\]]*)\]\s+\{([^}]*)\}\s+/(.*)/\s*(?:#.*)?$")


def source_map():
    """(trad, jyut) -> list of "/"-split senses (only entries with >1 sense)."""
    m = {}
    with open(SRC, encoding="utf-8") as f:
        for raw in f:
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            mt = LINE.match(line)
            if not mt:
                continue
            trad, _simp, _pin, jyut, blob = mt.groups()
            glosses = [g for g in blob.split("/") if g]
            if len(glosses) > 1:
                m[(trad, jyut.strip())] = glosses
    return m


def main():
    conn = sqlite3.connect(DB)
    src = source_map()
    next_sense = conn.execute("SELECT COALESCE(MAX(id),0) FROM sense").fetchone()[0]
    fixed = 0
    skipped_no_src = 0
    rows = conn.execute("SELECT id, headword, reading FROM lexeme WHERE variety='yue'").fetchall()
    for lid, head, reading in rows:
        glosses = src.get((head, (reading or "").strip()))
        if not glosses:
            skipped_no_src += 1
            continue
        senses = conn.execute(
            "SELECT id, gloss_en FROM sense WHERE lexeme_id=? ORDER BY sense_order", (lid,)
        ).fetchall()
        # only repair the collapsed single-row case that matches this source line exactly
        if len(senses) != 1:
            continue
        if senses[0][1] != "; ".join(glosses):
            continue
        conn.execute("DELETE FROM sense WHERE lexeme_id=?", (lid,))
        new = []
        for order, g in enumerate(glosses):
            next_sense += 1
            new.append((next_sense, lid, None, g, order))
        conn.executemany(
            "INSERT INTO sense(id,lexeme_id,pos,gloss_en,sense_order) VALUES (?,?,?,?,?)", new
        )
        fixed += 1
    conn.commit()
    # rebuild the gloss FTS so the new sense rows are searchable, then refresh stats
    try:
        conn.execute("INSERT INTO gloss_fts(gloss_fts) VALUES('rebuild')")
    except sqlite3.OperationalError as e:
        print(f"  (gloss_fts rebuild skipped: {e})", file=sys.stderr)
    conn.execute("ANALYZE")
    conn.commit()
    conn.close()
    print(f"canto sense re-split: fixed={fixed}, yue-without-source-match={skipped_no_src}")


if __name__ == "__main__":
    main()
