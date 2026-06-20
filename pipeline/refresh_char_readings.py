"""Repopulate char_reading pinyin/jyutping with the FULL polyphonic reading sets, in place.

The old ingest read only Unihan kMandarin/kCantonese, which give a single customary reading, so
polyphonic characters showed just one Mandarin/Cantonese reading (行 only xíng / hang4). This refresher
re-parses the richer Unihan fields (kTGHZ2013/kHanyuPinyin, kSMSZD2003Readings) and rewrites the
pinyin + jyutping rows with an `ord` so the customary reading still sorts first. Idempotent; leaves
on/kun/mc rows untouched. Rebuilds nothing else.

Usage: KOGU_DB=data/kogu.sqlite pipeline/.venv/bin/python pipeline/refresh_char_readings.py
"""
import os
import sqlite3
import sys
import zipfile
from collections import defaultdict

sys.path.insert(0, os.path.join(os.path.dirname(__file__)))
from kogupipe.ingest.cjk_readings import READING_SOURCE_FIELDS, parse_jyutping, parse_pinyin  # noqa: E402

DB = os.environ.get("KOGU_DB", "data/kogu.sqlite")
UNIHAN = "pipeline/sources/Unihan.zip"


def load_fields():
    fields = defaultdict(lambda: defaultdict(list))
    with zipfile.ZipFile(UNIHAN) as z:
        for line in z.read("Unihan_Readings.txt").decode("utf-8").splitlines():
            if not line or line[0] == "#":
                continue
            cpx, field, value = line.split("\t", 2)
            if field in READING_SOURCE_FIELDS:
                fields[int(cpx[2:], 16)][field].append(value)
    return fields


def main():
    conn = sqlite3.connect(DB)
    # add the ord column if this DB predates it
    cols = {r[1] for r in conn.execute("PRAGMA table_info(char_reading)")}
    if "ord" not in cols:
        conn.execute("ALTER TABLE char_reading ADD COLUMN ord INTEGER NOT NULL DEFAULT 0")

    valid = {r[0] for r in conn.execute("SELECT cp FROM character")}
    fields = load_fields()

    rows = []
    chars_with_multi = 0
    for cp in valid:
        rf = fields.get(cp)
        if not rf:
            continue
        py = parse_pinyin(rf)
        jy = parse_jyutping(rf)
        if len(py) > 1 or len(jy) > 1:
            chars_with_multi += 1
        for i, v in enumerate(py):
            rows.append((cp, "pinyin", v, i))
        for i, v in enumerate(jy):
            rows.append((cp, "jyutping", v, i))

    # replace only the pinyin/jyutping rows; on/kun/mc are left exactly as they are
    conn.execute("DELETE FROM char_reading WHERE kind IN ('pinyin','jyutping')")
    conn.executemany(
        "INSERT OR IGNORE INTO char_reading(cp,kind,value,ord) VALUES (?,?,?,?)", rows
    )
    conn.execute("ANALYZE")
    conn.commit()
    n_py = conn.execute("SELECT COUNT(*) FROM char_reading WHERE kind='pinyin'").fetchone()[0]
    n_jy = conn.execute("SELECT COUNT(*) FROM char_reading WHERE kind='jyutping'").fetchone()[0]
    conn.close()
    print(f"char_reading refresh: pinyin rows={n_py}, jyutping rows={n_jy}, "
          f"chars with >1 reading={chars_with_multi}")


if __name__ == "__main__":
    main()
