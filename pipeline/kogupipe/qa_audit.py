"""QA audit - flag dictionary entries that a human would instantly see as "wrong", so problems are
found automatically instead of by chance. DB-only heuristics (no app/render dependency), so it's a
cheap standing check. Two high-precision classes (from the QA investigation):

  C1  a single character used in Japanese (Kanjidic on/kun + a Japanese meaning) that has NO Japanese
      word-lexeme - it depends on the synthesized 日本語 row to appear co-equally. If that logic
      regresses, these silently become Chinese-only again. (Was ~1,000 chars before the fix.)
  C4b a single character whose ONLY definition in a variety is a bare cross-reference ("variant of X",
      "used in Y", surname) - a dead-end entry with no meaning of its own.

Run:  cd pipeline && .venv/bin/python -m kogupipe.qa_audit [db_path] [--limit N]
"""
from __future__ import annotations

import os
import re
import sqlite3
import sys

from .db import DB_PATH

_MINOR = re.compile(r"^(surname\b|old variant of|variant of|used in|see\b|abbr\b)", re.I)
_BRACKET = re.compile(r"\[[^\]]*\]")
_PIPE = re.compile(r"([^\s;|]+)\|[^\s;|]+")


def _clean(g: str) -> str:
    g = _BRACKET.sub("", g or "")
    g = re.sub(r"\(bound form\)\s*", "", g, flags=re.I)
    g = _PIPE.sub(r"\1", g)
    return g.strip()


def _is_minor(g: str) -> bool:
    s = _clean(g).lower()
    return not s or bool(_MINOR.match(s)) or "kangxi radical" in s or "radical in chinese characters" in s


def audit(conn: sqlite3.Connection, limit: int = 40):
    # C4b - per (variety, glyph), flag when EVERY sense across ALL its single-char lexemes is a
    # cross-reference (so 都/和 with a real "capital"/"peace" sense alongside a surname lexeme are NOT
    # flagged; the render merges same-reading lexemes and sinks the minor ones).
    from collections import OrderedDict

    groups: "OrderedDict[tuple[str, str], list[str]]" = OrderedDict()
    for lid, variety, hw in conn.execute(
        "SELECT l.id, l.variety, l.headword FROM lexeme l "
        "WHERE length(l.headword)=1 ORDER BY l.freq IS NULL, l.freq DESC"
    ):
        senses = [s for (s,) in conn.execute("SELECT gloss_en FROM sense WHERE lexeme_id=?", (lid,))]
        groups.setdefault((variety, hw), []).extend(senses)
    c4b = [
        (hw, variety, "; ".join(_clean(s) for s in senses)[:60])
        for (variety, hw), senses in groups.items()
        if senses and all(_is_minor(s) for s in senses)
    ]

    # C1 - a character actually in use (has a zh/yue word) and used in Japanese (Kanjidic on/kun + a
    # Japanese meaning) but with NO Japanese word-lexeme: it relies on the synthesized 日本語 row to
    # appear co-equally. A regression in that logic would silently make these Chinese-only again.
    c1: list[tuple[str, str]] = []
    for cp, ch, gja in conn.execute(
        "SELECT c.cp, c.char, c.gloss_ja FROM character c "
        "WHERE c.gloss_ja IS NOT NULL "
        "AND EXISTS (SELECT 1 FROM char_reading r WHERE r.cp=c.cp AND r.kind IN ('onyomi','kunyomi')) "
        "AND EXISTS (SELECT 1 FROM surface_form sf JOIN lexeme l ON l.id=sf.lexeme_id "
        "            WHERE sf.form=c.char AND l.variety IN ('zh','yue'))"
    ):
        has_ja = conn.execute(
            "SELECT 1 FROM surface_form sf JOIN lexeme l ON l.id=sf.lexeme_id "
            "WHERE sf.form=? AND l.variety='ja' LIMIT 1",
            (ch,),
        ).fetchone()
        if not has_ja:
            c1.append((ch, (gja or "")[:50]))

    return c1, c4b


def main(argv: list[str]) -> int:
    path = next((a for a in argv if not a.startswith("--")), os.environ.get("KOGU_DB", str(DB_PATH)))
    limit = 40
    if "--limit" in argv:
        limit = int(argv[argv.index("--limit") + 1])
    conn = sqlite3.connect(path)
    c1, c4b = audit(conn, limit)
    print(f"QA audit of {path}\n")
    print(f"C1  synth-dependent Japanese chars (used in JP, no ja lexeme): {len(c1)}")
    for ch, g in c1[:limit]:
        print(f"      {ch}  {g}")
    print(f"\nC4b dead-end entries (only sense is a cross-reference): {len(c4b)}")
    for ch, v, g in c4b[:limit]:
        print(f"      {ch} [{v}]  {g}")
    conn.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
