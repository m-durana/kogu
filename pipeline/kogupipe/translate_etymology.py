"""Batch-translate the non-English etymology (Chinese idiom 出處 + native zh/ja Wiktionary 詞源/語源)
into English via the app's own /mt endpoint (Google gtx, MyMemory fallback), storing it in
etymology.text_en. Resumable (skips rows already translated) and dedup'd by (source-lang, text).
Throttled so it doesn't get the server IP rate-limited (which would break the live translate feature).

Run:  cd pipeline && python3 -m kogupipe.translate_etymology [db_path]
"""
from __future__ import annotations

import json
import os
import sqlite3
import sys
import time
import urllib.parse
import urllib.request

MT = "http://127.0.0.1:4100/mt"
THROTTLE = 0.4  # seconds between live requests (dedup skips repeats)


def _mt(text: str, sl: str) -> str | None:
    url = MT + "?" + urllib.parse.urlencode({"q": text[:4800], "sl": sl})
    try:
        with urllib.request.urlopen(url, timeout=25) as r:
            return json.load(r).get("translation") or None
    except Exception:
        return None


def main(argv: list[str]) -> int:
    db = argv[0] if argv else os.environ.get("KOGU_DB", "/mnt/HC_Volume_102319212/kogu/kogu.sqlite")
    conn = sqlite3.connect(db)
    rows = conn.execute(
        "SELECT e.lexeme_id, e.text, e.source, l.variety FROM etymology e JOIN lexeme l ON l.id = e.lexeme_id "
        "WHERE e.source IN ('xinhua','mapull','wiktionary-native') AND e.text_en IS NULL"
    ).fetchall()
    print(f"to translate: {len(rows)}", flush=True)
    cache: dict[tuple[str, str], str | None] = {}
    done = fail = 0
    for i, (lid, text, source, variety) in enumerate(rows):
        sl = "ja" if source == "wiktionary-native" and variety == "ja" else "zh"
        key = (sl, text)
        if key not in cache:
            cache[key] = _mt(text, sl)
            time.sleep(THROTTLE)
        tr = cache[key]
        if tr:
            conn.execute("UPDATE etymology SET text_en=? WHERE lexeme_id=?", (tr, lid))
            done += 1
        else:
            fail += 1
        if (i + 1) % 200 == 0:
            conn.commit()
            print(f"  {i+1}/{len(rows)}  translated={done} failed={fail} uniq={len(cache)}", flush=True)
    conn.commit()
    conn.close()
    print(f"DONE: translated={done} failed={fail}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
