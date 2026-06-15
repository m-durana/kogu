"""Phase 3.2 — lexical origin badges + etymology passthrough (DESIGN.md §4.1).

Loads the compact extracts produced by `extract_etymology.py` (if present) and attaches them to
lexemes: a Wiktionary etymology paragraph (verbatim, no LLM) + origin badges (wasei-kango,
borrowed-from-japanese, calque, phono-semantic-matching). Skips silently if extracts are absent,
so a build works before the (rare, ~1.5 GB) extraction has been run.

Chinese Wiktionary is unified, so zh records match both zh and yue lexemes by form.
"""
from __future__ import annotations

import json
from collections import defaultdict

from ..db import SOURCES_DIR

LANG_TO_VARIETIES = {"zh": ("zh", "yue"), "ja": ("ja",)}


def ingest(conn) -> None:
    # (variety, form) -> [lexeme_id]
    index: dict[tuple[str, str], list[int]] = defaultdict(list)
    for lid, variety, form in conn.execute(
        "SELECT l.id, l.variety, sf.form FROM surface_form sf JOIN lexeme l ON l.id = sf.lexeme_id"
    ):
        index[(variety, form)].append(lid)

    ety_rows: dict[int, tuple[int, str]] = {}  # lexeme_id -> (lexeme_id, text)
    badge_rows: set[tuple[int, str]] = set()
    files = 0
    for lang, varieties in LANG_TO_VARIETIES.items():
        path = SOURCES_DIR / f"etymology.{lang}.jsonl"
        if not path.exists():
            continue
        files += 1
        for raw in path.read_text(encoding="utf-8").splitlines():
            if not raw:
                continue
            rec = json.loads(raw)
            word, ety, badges = rec.get("word"), rec.get("ety") or "", rec.get("badges") or []
            for variety in varieties:
                for lid in index.get((variety, word), ()):
                    if ety and lid not in ety_rows:
                        ety_rows[lid] = (lid, ety)
                    for b in badges:
                        badge_rows.add((lid, b))

    if not files:
        print("      (no etymology extracts found — run kogupipe.extract_etymology; skipping)")
        return

    conn.executemany(
        "INSERT OR IGNORE INTO etymology(lexeme_id,text,source) VALUES (?,?, 'wiktionary')",
        ety_rows.values())
    conn.executemany(
        "INSERT OR IGNORE INTO origin_badge(lexeme_id,badge) VALUES (?,?)", sorted(badge_rows))
    print(f"      etymologies={len(ety_rows)} origin-badges={len(badge_rows)}")
