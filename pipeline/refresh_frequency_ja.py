"""Rescore Japanese word frequency with wordfreq, guarding against composed scores, in place.

Every ja lexeme carried a JMdict-derived freq, and those are badly calibrated: 加番 (Edo castle
guards) scored above 鞄 (bag), so common lookups surfaced museum pieces first. wordfreq has good
Japanese data, but its zipf_frequency COMPOSES a score for any unknown compound from its pieces
(加番 -> 加+番 -> a fake 4.31, higher than the genuine 鞄 3.83), so it can only be trusted when
the word is a single token of its tokenizer.

Rule per lexeme: best zipf over its non-rare FORMS where tokenize(form) == [form]; if no form
qualifies, the kana reading's zipf x 0.85 (readings are shared by homophones, so they rank a word
family, not the word). Lexemes where neither applies keep their JMdict score.

Idempotent; rerun after any ja re-ingest. Restart kogu.service afterwards (freq is cached).

Usage: KOGU_DB=data/kogu.sqlite pipeline/.venv/bin/python pipeline/refresh_frequency_ja.py
"""
from __future__ import annotations

import os
import sqlite3

from wordfreq import tokenize, zipf_frequency

DB = os.environ.get("KOGU_DB", "data/kogu.sqlite")
READING_PENALTY = 0.85
# mirrors kogupipe.ingest.frequency._zipf_to_score
ZIPF_LO, ZIPF_HI = 1.0, 7.3


def zipf_to_score(z: float) -> float | None:
    if z <= 0:
        return None
    return max(0.0, min(1.0, (z - ZIPF_LO) / (ZIPF_HI - ZIPF_LO)))


def single_token_zipf(w: str) -> float | None:
    try:
        if tokenize(w, "ja") != [w]:
            return None
    except Exception:
        return None
    z = zipf_frequency(w, "ja")
    return z if z > 0 else None


def main() -> None:
    conn = sqlite3.connect(DB)
    forms: dict[int, list[str]] = {}
    for lid, form in conn.execute(
        "SELECT s.lexeme_id, s.form FROM surface_form s JOIN lexeme l ON l.id=s.lexeme_id "
        "WHERE l.variety='ja' AND s.rare=0"
    ):
        forms.setdefault(lid, []).append(form)

    cache: dict[str, float | None] = {}

    def zf(w: str) -> float | None:
        if w not in cache:
            cache[w] = single_token_zipf(w)
        return cache[w]

    updates = []
    n_form = n_read = 0
    for lid, reading in conn.execute("SELECT id, reading FROM lexeme WHERE variety='ja'"):
        best = None
        for f in forms.get(lid, []):
            z = zf(f)
            if z is not None and (best is None or z > best):
                best = z
        if best is not None:
            n_form += 1
        elif reading:
            z = zf(reading)
            if z is not None:
                best = z * READING_PENALTY
                n_read += 1
        if best is not None:
            score = zipf_to_score(best)
            if score is not None:
                updates.append((score, lid))
    conn.executemany("UPDATE lexeme SET freq=?1, freq_source='wordfreq-ja' WHERE id=?2", updates)
    # Words wordfreq has NEVER seen are, by construction, rare: their leftover JMdict scores
    # (median ~0.6) would outrank the genuinely-common rescored words (median ~0.25). Squash them
    # rank-preserved into [0, 0.25], keeping JMdict's internal ordering as the tiebreak within
    # the rare band. Source tag guards idempotence.
    squashed = conn.execute(
        "UPDATE lexeme SET freq = freq * 0.25, freq_source = 'jmdict-lowband' "
        "WHERE variety='ja' AND freq_source='jmdict' AND freq IS NOT NULL"
    ).rowcount
    conn.commit()
    print(
        f"rescored {len(updates)} ja lexemes (form-scored {n_form}, reading-fallback {n_read}); "
        f"squashed {squashed} wordfreq-unknown words into the rare band"
    )


if __name__ == "__main__":
    main()
