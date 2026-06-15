"""Word-frequency ingest — populates lexeme.freq so search ranks common words first.

Without this, every lexeme has freq=1.0 and ranking ties break arbitrarily (e.g. toneless "ren"
surfaces a rare 肕 above 人). Sources are OpenSubtitles word-frequency lists (hermitdave/
FrequencyWords, permissive): Chinese (simplified) and Japanese. Cantonese has no list, so it
borrows the Chinese scores for its shared written vocabulary (粵字 fall back to the baseline).

freq is a score in (0,1]: rank 1 ≈ 1.0, rank 50000 ≈ 0.15 (log scale). Lexemes with no match
stay NULL and the serving layer gives them a low baseline, so any signal beats no signal.
"""
import math

from ..db import SOURCES_DIR

NMAX = 50000
FLOOR = 0.15  # score for the least-frequent ranked word (still > the serving baseline for unmatched)


def _load_ranked(path) -> dict[str, float]:
    """word -> score, from a 'word count' per-line list (already frequency-ordered)."""
    out: dict[str, float] = {}
    if not path.exists():
        return out
    for rank, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        word = line.split(" ", 1)[0].strip()
        if not word or word in out:
            continue
        norm = 1.0 - math.log(rank) / math.log(NMAX + 1)
        out[word] = round(FLOOR + (1.0 - FLOOR) * max(0.0, norm), 4)
    return out


def _simp_to_trad(conn) -> dict[str, str]:
    """char-level simplified→traditional (first option), so the Chinese (simplified) list can also
    score traditional forms — used by zh traditional surface forms and by Cantonese."""
    m: dict[str, str] = {}
    for child, parent in conn.execute(
        "SELECT child_cp, parent_cp FROM glyph_edge WHERE type='simplification'"
    ):
        m.setdefault(chr(child), chr(parent))
    return m


def _to_trad(word: str, s2t: dict[str, str]) -> str:
    return "".join(s2t.get(c, c) for c in word)


def ingest(conn) -> None:
    zh = _load_ranked(SOURCES_DIR / "freq_zh_cn.txt")
    ja = _load_ranked(SOURCES_DIR / "freq_ja.txt")
    if not zh and not ja:
        print("    (no frequency lists found — skipping)")
        return

    # widen the Chinese map with traditional spellings so trad zh forms and Cantonese also match
    s2t = _simp_to_trad(conn)
    zh_wide = dict(zh)
    for word, score in zh.items():
        trad = _to_trad(word, s2t)
        if trad != word:
            zh_wide.setdefault(trad, score)

    def best_for(variety: str, forms: list[str], kana: str | None) -> float | None:
        score: float | None = None
        maps = [ja] if variety == "ja" else [zh_wide]
        keys = list(forms) + ([kana] if (variety == "ja" and kana) else [])
        for m in maps:
            for k in keys:
                if k in m and (score is None or m[k] > score):
                    score = m[k]
        return score

    rows = conn.execute("SELECT id, variety, reading FROM lexeme").fetchall()
    forms_by_lex: dict[int, list[str]] = {}
    for lex, form in conn.execute("SELECT lexeme_id, form FROM surface_form"):
        forms_by_lex.setdefault(lex, []).append(form)

    updates = []
    for lex, variety, reading in rows:
        s = best_for(variety, forms_by_lex.get(lex, []), reading)
        if s is not None:
            updates.append((s, lex))
    conn.executemany("UPDATE lexeme SET freq=?1 WHERE id=?2", updates)
    print(f"    frequency: {len(updates)}/{len(rows)} lexemes scored "
          f"(zh {len(zh)}, ja {len(ja)} words)")
