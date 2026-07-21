"""Howell Etymological Dictionary of Han/Chinese Characters (MIT) - character-level etymology.

Gap-fills real, sourced phono-semantic CHARACTER etymologies where Wiktionary has none. Keyed on
single-character surface forms, so one Howell entry (古) attaches to every zh / yue / ja lexeme
written with that glyph. INSERT OR IGNORE, so existing Wiktionary etymology is never overwritten -
this only fills the ~40% of single characters that had no etymology at all.

Source: Lawrence J. Howell / Hikaru Morimoto, MIT-licensed
(github.com/conscientiousCode/Etymological-Dictionary-of-Han-Chinese-Characters-Database).
File format: entries delimited by `___`; each block's first line is `字　(strokes)　readings`,
the rest is the etymology prose. The leading copyright block has no CJK head char and is skipped.
"""
from __future__ import annotations

from ..db import SOURCES_DIR


def _is_cjk(c: str) -> bool:
    o = ord(c)
    return 0x3400 <= o <= 0x9FFF or 0xF900 <= o <= 0xFAFF or 0x20000 <= o <= 0x2FFFF


def _parse() -> dict[str, str]:
    path = SOURCES_DIR / "howell_etymology.txt"
    if not path.exists():
        return {}
    out: dict[str, str] = {}
    for block in path.read_text(encoding="utf-8").split("___"):
        lines = [ln.strip() for ln in block.strip().splitlines() if ln.strip()]
        if not lines:
            continue
        head = lines[0]
        ch = head[0]
        if len(ch) != 1 or not _is_cjk(ch):
            continue  # copyright preamble / non-character block
        ety = " ".join(lines[1:]).strip()
        if ety:
            out.setdefault(ch, ety)
    return out


def ingest(conn) -> None:
    data = _parse()
    if not data:
        print("      (sources/howell_etymology.txt not found; skipping Howell etymology)")
        return
    rows = [
        (lid, data[form])
        for lid, form in conn.execute(
            "SELECT l.id, sf.form FROM surface_form sf JOIN lexeme l ON l.id = sf.lexeme_id "
            "WHERE length(sf.form) = 1"
        )
        if form in data
    ]
    conn.executemany(
        "INSERT OR IGNORE INTO etymology(lexeme_id,text,source) VALUES (?,?, 'howell')", rows
    )
    print(f"      howell char etymologies: {len(data)} glyphs, gap-filled across {len(rows)} single-char lexemes")
