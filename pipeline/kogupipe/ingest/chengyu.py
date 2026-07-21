"""chinese-xinhua idiom etymology (MIT) - the classical source (出處) of a 成語.

Gap-fills WORD etymology for four-character idioms, which almost always have a documented literary
source (語出《...》) that the English Wiktionary usually lacks. INSERT OR IGNORE, so existing
etymology is never overwritten. Chinese Wiktionary is unified, so a Mandarin idiom form also matches
its Cantonese lexeme. Source: pwxcoo/chinese-xinhua (MIT), data/idiom.json.
"""
from __future__ import annotations

import json

from ..db import SOURCES_DIR


def _parse() -> dict[str, str]:
    path = SOURCES_DIR / "idiom.json"
    if not path.exists():
        return {}
    out: dict[str, str] = {}
    for x in json.loads(path.read_text(encoding="utf-8")):
        w = (x.get("word") or "").strip()
        d = (x.get("derivation") or "").strip().strip("”“\"").strip()
        if w and d and d not in ("无", "無", "無。", "无。") and len(w) >= 2:
            out.setdefault(w, d)
    return out


def ingest(conn) -> None:
    data = _parse()
    if not data:
        print("      (sources/idiom.json not found; skipping chengyu etymology)")
        return
    rows = [
        (lid, data[form])
        for lid, form in conn.execute(
            "SELECT l.id, sf.form FROM surface_form sf JOIN lexeme l ON l.id = sf.lexeme_id "
            "WHERE l.variety IN ('zh','yue') AND length(sf.form) >= 2"
        )
        if form in data
    ]
    conn.executemany(
        "INSERT OR IGNORE INTO etymology(lexeme_id,text,source) VALUES (?,?, 'xinhua')", rows
    )
    print(f"      chengyu (chinese-xinhua) etymologies: {len(data)} idioms, gap-filled across {len(rows)} lexemes")
