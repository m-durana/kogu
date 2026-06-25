"""Confusable look-alike characters from Unihan ``kSpoofingVariant`` (homoglyphs usable for spoofing:
日/曰, 未/末, 土/士). This is purely a VISUAL confusability signal — not identity, not shared meaning —
so it is kept entirely out of the variant graph (glyph_edge) and only surfaced as an "easily confused
with" note. kSpoofingVariant is symmetric, so both directions are stored.
"""
from __future__ import annotations

from .backbone import _unihan


def ingest(conn) -> None:
    chars = {cp for (cp,) in conn.execute("SELECT cp FROM character")}
    pairs: set[tuple[int, int]] = set()
    for cp, field, value in _unihan("Unihan_Variants.txt"):
        if field != "kSpoofingVariant" or cp not in chars:
            continue
        for tok in value.split():
            code = tok.split("<")[0]  # defensive: strip any source annotation
            if not code.startswith("U+"):
                continue
            try:
                other = int(code[2:], 16)
            except ValueError:
                continue
            if other in chars and other != cp:
                pairs.add((cp, other))
                pairs.add((other, cp))  # symmetric
    conn.executemany(
        "INSERT OR IGNORE INTO char_confusable(cp, confusable_cp) VALUES (?,?)", sorted(pairs)
    )
    print(f"      confusables={len(pairs)} directed pairs")
