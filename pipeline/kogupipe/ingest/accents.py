"""Japanese pitch-accent (Kanjium accents.txt, CC BY-SA 4.0) onto ja kana readings.

Kanjium's accents.txt is tab-separated `kanji<TAB>kana<TAB>accent`:
  橋  はし  2      下降 after mora 2 (odaka)
  箸  はし  1      下降 after mora 1 (atamadaka)
  端  はし  0      no downstep (heiban)
  寿司 すし  2,1    a word with two attested accents (we keep the list, serve the first)
  あ      1        a kana-only word: the FIRST field is itself the kana (kana field is empty)
Some accent cells carry part-of-speech tags, e.g. "(副)0,(名)3"; we strip the tags and keep the
numeric downstep list in order.

The accent is keyed per (kanji headword, kana reading) — the same grain as our ja kana readings —
so we match a ja lexeme's KANJI surface form + kana value first (disambiguating homographs:
箸/はし=1 vs 橋/はし=2 vs 端/はし=0), then fall back to a kana-only map for words written in kana.

Idempotent: ALTERs in the nullable `accent` column if missing, clears prior ja-kana accents, repopulates,
and stamps build_meta('kanjium_accent'). Safe to run twice (the second run reproduces the same state).

Run as a live-DB refresher:  cd pipeline && .venv/bin/python -m kogupipe.apply_accents [db_path]
"""
from __future__ import annotations

import re

from ..db import SOURCES_DIR

SRC = SOURCES_DIR / "accents.txt"
VERSION = "1"  # bump when the parse rules or source change

_NUM = re.compile(r"\d+")


def _clean_accent(cell: str) -> str | None:
    """Strip POS tags ("(副)0,(名)3" → "0,3"), keep the numeric downstep indices in order.
    Returns None when the cell holds no number."""
    nums = _NUM.findall(cell)
    return ",".join(nums) if nums else None


def parse(text: str) -> tuple[dict[tuple[str, str], str], dict[str, str]]:
    """Build (kanji, kana) -> accent and kana-only -> accent maps from accents.txt content.

    First-wins on duplicate keys: accents.txt lists its commonest form first, so an earlier row is
    kept rather than overwritten (deterministic and stable across re-runs)."""
    by_pair: dict[tuple[str, str], str] = {}
    by_kana: dict[str, str] = {}
    for raw in text.splitlines():
        if not raw:
            continue
        parts = raw.split("\t")
        if len(parts) < 3:
            continue
        kanji, kana, accent_cell = parts[0], parts[1], parts[2]
        accent = _clean_accent(accent_cell)
        if accent is None:
            continue
        if kana:  # kanji form + kana reading
            by_pair.setdefault((kanji, kana), accent)
        else:  # kana-only word: field 1 IS the kana
            by_kana.setdefault(kanji, accent)
    return by_pair, by_kana


def _ensure_column(conn) -> None:
    cols = {r[1] for r in conn.execute("PRAGMA table_info(lexeme_reading)")}
    if "accent" not in cols:
        conn.execute("ALTER TABLE lexeme_reading ADD COLUMN accent TEXT")


def ingest(conn) -> None:
    if not SRC.exists():
        print("      accents.txt missing - skipping (Kanjium pitch accent)")
        return
    _ensure_column(conn)
    by_pair, by_kana = parse(SRC.read_text(encoding="utf-8"))

    # idempotent: clear any prior ja-kana accents so re-running reproduces exactly this state
    conn.execute(
        "UPDATE lexeme_reading SET accent=NULL WHERE kind='kana' AND accent IS NOT NULL "
        "AND lexeme_id IN (SELECT id FROM lexeme WHERE variety='ja')"
    )

    # each ja lexeme: its kanji (non-kana) surface forms + its kana readings
    kanji_forms: dict[int, list[str]] = {}
    for lid, form in conn.execute(
        "SELECT sf.lexeme_id, sf.form FROM surface_form sf JOIN lexeme l ON l.id=sf.lexeme_id "
        "WHERE l.variety='ja' AND sf.script<>'kana'"
    ):
        kanji_forms.setdefault(lid, []).append(form)

    updates: list[tuple[str, int, str]] = []  # (accent, lexeme_id, kana)
    total = matched = 0
    for lid, kana in conn.execute(
        "SELECT lr.lexeme_id, lr.value FROM lexeme_reading lr JOIN lexeme l ON l.id=lr.lexeme_id "
        "WHERE l.variety='ja' AND lr.kind='kana'"
    ):
        total += 1
        accent = None
        for kf in kanji_forms.get(lid, ()):  # kanji form + kana disambiguates homographs
            accent = by_pair.get((kf, kana))
            if accent is not None:
                break
        if accent is None and lid not in kanji_forms:  # kana-only word: kana fallback
            accent = by_kana.get(kana)
        if accent is not None:
            updates.append((accent, lid, kana))
            matched += 1

    conn.executemany(
        "UPDATE lexeme_reading SET accent=?1 WHERE lexeme_id=?2 AND kind='kana' AND value=?3", updates
    )
    conn.execute(
        "INSERT OR REPLACE INTO build_meta(key,value) VALUES ('kanjium_accent',?)", (VERSION,)
    )
    rate = (matched / total * 100) if total else 0.0
    print(
        f"      kanjium accent: {matched}/{total} ja kana readings got an accent ({rate:.1f}%) "
        f"from {len(by_pair)} kanji-kana pairs + {len(by_kana)} kana-only entries"
    )
