"""Phase 3.2 - lexical origin badges + etymology passthrough (DESIGN.md §4.1).

Loads the compact extracts produced by `extract_etymology.py` (if present) and attaches them to
lexemes: a Wiktionary etymology paragraph (verbatim, no LLM) + origin badges (wasei-kango,
borrowed-from-japanese, calque, phono-semantic-matching). Skips silently if extracts are absent,
so a build works before the (rare, ~1.5 GB) extraction has been run.

Chinese Wiktionary is unified, so zh records match both zh and yue lexemes by form.
"""
from __future__ import annotations

import json
import re
from collections import defaultdict

from ..db import SOURCES_DIR

LANG_TO_VARIETIES = {"zh": ("zh", "yue"), "ja": ("ja",)}

# an English borrowing names its source word; used to route the etymon to the right homograph
_ENG_BORROW = re.compile(r"\b(?:from|borrowed from|abbreviation of)\s+english\s+([a-z]{3,})", re.I)


def ingest(conn) -> None:
    # (variety, form) -> [lexeme_id]
    index: dict[tuple[str, str], list[int]] = defaultdict(list)
    for lid, variety, form in conn.execute(
        "SELECT l.id, l.variety, sf.form FROM surface_form sf JOIN lexeme l ON l.id = sf.lexeme_id"
    ):
        index[(variety, form)].append(lid)

    # first-sense gloss per lexeme, to keep a borrowing's etymon off same-spelling homographs it
    # doesn't belong to (サイン "signature" must not inherit ムース-style "From English sine").
    gloss: dict[int, str] = {}
    for lid, g in conn.execute(
        "SELECT lexeme_id, group_concat(lower(gloss_en),' ') FROM sense GROUP BY lexeme_id"
    ):
        gloss[lid] = g or ""

    def owns(lid: int, word: str) -> bool:
        return re.search(r"\b" + re.escape(word) + r"\b", gloss.get(lid, "")) is not None

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
                lids = index.get((variety, word), ())
                # route an English borrowing to the homograph whose own meaning IS the source word;
                # if none matches, skip the text rather than smear it onto every same-spelling lexeme.
                ety_targets = lids
                m = _ENG_BORROW.search(ety) if ety else None
                if m and len(lids) > 1:
                    ety_targets = [lid for lid in lids if owns(lid, m.group(1).lower())]
                for lid in lids:
                    if ety and lid in ety_targets and lid not in ety_rows:
                        ety_rows[lid] = (lid, ety)
                    for b in badges:
                        badge_rows.add((lid, b))

    if not files:
        print("      (no etymology extracts found - run kogupipe.extract_etymology; skipping)")
        return

    conn.executemany(
        "INSERT OR IGNORE INTO etymology(lexeme_id,text,source) VALUES (?,?, 'wiktionary')",
        ety_rows.values())
    conn.executemany(
        "INSERT OR IGNORE INTO origin_badge(lexeme_id,badge) VALUES (?,?)", sorted(badge_rows))
    print(f"      etymologies={len(ety_rows)} origin-badges={len(badge_rows)}")
