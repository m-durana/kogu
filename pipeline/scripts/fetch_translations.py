#!/usr/bin/env python3
"""STREAM-AND-FILTER the English Wiktionary (kaikki/wiktextract) dump into a COMPACT translations
TSV - WITHOUT ever landing the ~20GB decompressed dump (or the ~470MB .gz) on disk.

It pipes the gz over HTTP through Python's gzip reader line-by-line and writes only the zh/cmn/yue/ja
targets as `en_headword <TAB> lang <TAB> word`. A hard cap stops well before the disk could fill.

DISK SAFETY: the root disk is tiny; this writes ONLY the compact TSV (a few MB). The stream is
consumed in memory chunk-by-chunk and discarded. Output goes to pipeline/sources/ by default.

Run:  cd pipeline && .venv/bin/python scripts/fetch_translations.py [out.tsv]
"""
from __future__ import annotations

import gzip
import json
import sys
import urllib.request
from pathlib import Path

URL = "https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl.gz"
WANT = {"zh", "cmn", "yue", "ja"}

# hard caps - whichever trips first stops the stream (disk safety + time bound)
MAX_OUT_BYTES = 50 * 1024 * 1024   # stop once the compact TSV exceeds ~50MB
MAX_LINES = 5_000_000              # ... or after this many source entries


def main(argv: list[str]) -> int:
    out_path = Path(argv[0]) if argv else (
        Path(__file__).resolve().parents[1] / "sources" / "wiktionary_translations.tsv")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    print(f"streaming {URL}\n  -> {out_path}  (cap {MAX_OUT_BYTES // (1024*1024)}MB / {MAX_LINES} entries)")

    written = lines = entries_with_tr = 0
    out_bytes = 0
    req = urllib.request.Request(URL, headers={"User-Agent": "kogu-pipeline/1.0"})
    with urllib.request.urlopen(req) as resp, gzip.GzipFile(fileobj=resp) as gz, \
            out_path.open("w", encoding="utf-8") as out:
        out.write("# en\tlang\tword  (from English Wiktionary translation tables via kaikki)\n")
        for raw in gz:
            lines += 1
            if lines % 200_000 == 0:
                print(f"  ...{lines:,} entries, {written:,} translation rows, {out_bytes/1e6:.1f}MB")
            if lines > MAX_LINES or out_bytes > MAX_OUT_BYTES:
                print("  hard cap reached - stopping stream")
                break
            try:
                o = json.loads(raw)
            except Exception:
                continue
            tr = o.get("translations")
            if not tr:
                continue
            en = (o.get("word") or "").strip()
            if not en:
                continue
            had = False
            for t in tr:
                code = (t.get("code") or t.get("lang_code") or "").strip().lower()
                word = (t.get("word") or "").strip()
                if code in WANT and word and "\t" not in en and "\t" not in word:
                    row = f"{en}\t{code}\t{word}\n"
                    out.write(row)
                    out_bytes += len(row.encode("utf-8"))
                    written += 1
                    had = True
            entries_with_tr += had

    print(f"done: {lines:,} entries scanned, {entries_with_tr:,} with zh/cmn/yue/ja, "
          f"{written:,} rows written, {out_bytes/1e6:.2f}MB")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
