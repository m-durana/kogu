"""Fix is_orthodox for merger-target characters, in place.

The backbone marks a glyph non-orthodox whenever it is the child of a simplification/shinjitai
edge. That is wrong for characters that are ALSO ordinary traditional characters which other
characters merged INTO: 周 (週/賙 merged into it), 干 (乾/幹/榦), 后 (後), 面 (麪/麵), 里, 台,
只, 系, 云 ... and for modern-standard forms whose only graph parent is a kyūjitai variant
(為/爲, 衛, 煙, 並, 真 ...). With the flag wrong, the character page anchors its script family
on an arbitrary parent and claims e.g. "traditional 賙 → simplified 周", which is nonsense for
one of the most common characters in the language.

The discriminator is usage: a character that appears in the TRADITIONAL surface form of at
least MIN_TRAD_USES Chinese lexemes is a real traditional character, whatever the glyph graph
says. Japan-only shinjitai (円 効 亀 竜) appear on the traditional side only in a handful of
borrowed proper nouns and stay below the threshold, so they keep is_orthodox=0 and their
圓 → 円 family band.

Idempotent; touches only character.is_orthodox. Run after lexeme ingest (it needs the surface
forms), and again after any surface-form refresh.

Usage: KOGU_DB=data/kogu.sqlite python3 pipeline/refresh_orthodox.py
"""
import os
import sqlite3
from collections import Counter

DB = os.environ.get("KOGU_DB", "data/kogu.sqlite")
MIN_TRAD_USES = 5


def main():
    conn = sqlite3.connect(DB)
    trad_uses = Counter()
    for (form,) in conn.execute(
        "SELECT s.form FROM surface_form s JOIN lexeme l ON l.id = s.lexeme_id "
        "WHERE l.variety = 'zh' AND s.script = 'trad'"
    ):
        for ch in set(form):
            trad_uses[ch] += 1

    flips = [
        ch
        for (ch,) in conn.execute("SELECT char FROM character WHERE is_orthodox = 0")
        if trad_uses[ch] >= MIN_TRAD_USES
    ]
    conn.executemany("UPDATE character SET is_orthodox = 1 WHERE char = ?", [(c,) for c in flips])
    conn.commit()
    print(f"flipped {len(flips)} characters to orthodox: {''.join(sorted(flips))}")


if __name__ == "__main__":
    main()
