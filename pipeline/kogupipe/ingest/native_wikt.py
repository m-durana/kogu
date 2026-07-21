"""Native-edition Wiktionary etymology (kaikki zhwiktionary + jawiktionary).

The Chinese and Japanese Wiktionary editions carry 詞源/語源 sections on multi-character WORDS that
the English edition omits (邏輯 "音義兼譯自英語 logic…嚴復造出", たそがれ "＜誰そ彼"). Same wiktextract
schema we already use for the English kaikki dump, just a different edition; the field is
`etymology_texts` (a list). Same CC BY-SA + GFDL licence. Gap-fills word etymology (INSERT OR IGNORE).
The text is in the source language; English translation is handled separately, not here.
"""
from __future__ import annotations

import json

from ..db import SOURCES_DIR

# (file, varieties the edition's words map to)
EDITIONS = [("kaikki_zh.jsonl", ("zh", "yue")), ("kaikki_ja.jsonl", ("ja",))]


def _parse_file(name: str) -> dict[str, str]:
    path = SOURCES_DIR / name
    if not path.exists():
        return {}
    out: dict[str, str] = {}
    with path.open(encoding="utf-8") as f:
        for line in f:
            if "etymology_texts" not in line:
                continue
            try:
                r = json.loads(line)
            except ValueError:
                continue
            w = (r.get("word") or "").strip()
            ets = r.get("etymology_texts") or []
            if not w or len(w) < 2 or not ets:
                continue
            text = " ".join(t.strip() for t in ets if t and t.strip())
            if text:
                out.setdefault(w, text)
    return out


def ingest(conn) -> None:
    for name, varieties in EDITIONS:
        data = _parse_file(name)
        if not data:
            print(f"      (sources/{name} not found; skipping native-wiktionary)")
            continue
        ph = ",".join("?" * len(varieties))
        rows = [
            (lid, data[form])
            for lid, form in conn.execute(
                f"SELECT l.id, sf.form FROM surface_form sf JOIN lexeme l ON l.id = sf.lexeme_id "
                f"WHERE l.variety IN ({ph}) AND length(sf.form) >= 2",
                varieties,
            )
            if form in data
        ]
        conn.executemany(
            "INSERT OR IGNORE INTO etymology(lexeme_id,text,source) VALUES (?,?, 'wiktionary-native')", rows
        )
        print(f"      native wiktionary {name}: {len(data)} words w/ etymology, gap-filled {len(rows)} lexemes")
