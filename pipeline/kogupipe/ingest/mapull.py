"""mapull/chinese-dictionary idiom etymology (MIT) - 成語 出處 with the classical source cited.

A second, larger idiom set (its `source` field carries {book, text} = the cited work + quotation).
Gap-fills WORD etymology on top of chinese-xinhua ([[chengyu]]); INSERT OR IGNORE never overwrites.
Source: github.com/mapull/chinese-dictionary (MIT), idiom/idiom.json.
"""
from __future__ import annotations

import json

from ..db import SOURCES_DIR


def _parse() -> dict[str, str]:
    path = SOURCES_DIR / "mapull_idiom.json"
    if not path.exists():
        return {}
    out: dict[str, str] = {}
    for x in json.loads(path.read_text(encoding="utf-8")):
        w = (x.get("word") or "").strip()
        s = x.get("source") or {}
        book = (s.get("book") or "").strip()
        text = (s.get("text") or "").strip()
        if not w or (not book and not text) or book in ("无", "無"):
            continue
        ety = f"{book}：{text}" if book and text else (book or text)
        out.setdefault(w, ety)
    return out


def ingest(conn) -> None:
    data = _parse()
    if not data:
        print("      (sources/mapull_idiom.json not found; skipping mapull etymology)")
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
        "INSERT OR IGNORE INTO etymology(lexeme_id,text,source) VALUES (?,?, 'mapull')", rows
    )
    print(f"      mapull idiom etymologies: {len(data)} idioms, gap-filled across {len(rows)} lexemes")
