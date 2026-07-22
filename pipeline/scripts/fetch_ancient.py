"""Fetch ancient-script forms (oracle 甲骨文 / bronze 金文 / seal 篆書) for Han characters from
Wikimedia Commons and index them for the character-evolution panel.

Source: the "Ancient Chinese characters" project mirrors Richard Sears's forms as public-domain (CC0)
SVGs at the deterministic path `Special:FilePath/<glyph>-<period>.svg`. We save the tiny vector files
verbatim onto the scratch volume and record which periods exist per codepoint in `char_ancient`; the
backend serves them via /ancient/<cp>/<period> and the entry response lists the available periods.
Rare characters Commons doesn't cover are simply left without a panel (EVOBC fallback is a later step).

Run:  cd pipeline && .venv/bin/python scripts/fetch_ancient.py [max_chars]
Resumable (a `char_ancient_seen` marker skips already-scanned codepoints); polite serial fetch.
"""
from __future__ import annotations

import os
import sqlite3
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

DB = os.environ.get("KOGU_DB", "/srv/miro/kogu/data/kogu.sqlite")
DEST = "/mnt/HC_Volume_102319212/kogu/ancient"
PERIODS = ("oracle", "bronze", "seal")
UA = "kogu/1.0 (https://kogu.miro.build; mdurana@ethz.ch)"


def fetch(glyph: str, period: str) -> bytes | None:
    url = "https://commons.wikimedia.org/wiki/Special:FilePath/" + urllib.parse.quote(f"{glyph}-{period}.svg")
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    try:
        with urllib.request.urlopen(req, timeout=20) as r:
            if r.status == 200 and "svg" in (r.headers.get("Content-Type") or ""):
                data = r.read()
                # guard against a stray HTML error page slipping through
                return data if data.lstrip()[:5] in (b"<?xml", b"<svg ") or b"<svg" in data[:200] else None
    except urllib.error.HTTPError:
        return None
    except (urllib.error.URLError, TimeoutError):
        return None
    return None


def main(argv: list[str]) -> int:
    limit = int(argv[0]) if argv else 1_000_000
    conn = sqlite3.connect(DB)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS char_ancient ("
        " cp INTEGER NOT NULL, period TEXT NOT NULL, src TEXT NOT NULL, PRIMARY KEY (cp, period))"
    )
    conn.execute("CREATE TABLE IF NOT EXISTS char_ancient_seen (cp INTEGER PRIMARY KEY)")
    conn.commit()
    # most-looked-up characters first: single-char words by frequency, then the rest by codepoint
    rows = conn.execute(
        "SELECT c.cp, c.char FROM character c "
        "LEFT JOIN (SELECT unicode(headword) cp, MAX(freq) f FROM lexeme WHERE length(headword)=1 "
        "           GROUP BY unicode(headword)) L ON L.cp=c.cp "
        "WHERE c.cp NOT IN (SELECT cp FROM char_ancient_seen) "
        "ORDER BY L.f IS NULL, L.f DESC, c.cp LIMIT ?",
        (limit,),
    ).fetchall()
    scanned = hits = 0
    for cp, glyph in rows:
        got = []
        for period in PERIODS:
            data = fetch(glyph, period)
            if data is None:
                continue
            d = f"{DEST}/{cp}"
            os.makedirs(d, exist_ok=True)
            with open(f"{d}/{period}.svg", "wb") as f:
                f.write(data)
            got.append(period)
            time.sleep(0.15)
        for period in got:
            conn.execute(
                "INSERT OR REPLACE INTO char_ancient(cp,period,src) VALUES (?,?, 'commons')", (cp, period)
            )
        conn.execute("INSERT OR IGNORE INTO char_ancient_seen(cp) VALUES (?)", (cp,))
        scanned += 1
        hits += len(got)
        if scanned % 100 == 0:
            conn.commit()
            print(f"  scanned {scanned}, {hits} images so far", flush=True)
    conn.commit()
    total = conn.execute("SELECT COUNT(*) FROM char_ancient").fetchone()[0]
    conn.close()
    print(f"done: scanned {scanned} new chars, {hits} images this run; char_ancient now {total} rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
