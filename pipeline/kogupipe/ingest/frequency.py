"""Word-frequency ingest - populates lexeme.freq so search ranks common words first.

Primary source is the `wordfreq` library (Apache-2.0 code, CC-BY-SA-4.0 data): a multi-corpus blend
(Wikipedia + subtitles + web + news + books) on a uniform Zipf scale, far deeper than the old 50k
OpenSubtitles lists (zh coverage went from ~23% to most headwords). wordfreq accepts TRADITIONAL input
for 'zh' and unifies it, so our traditional headwords match directly with no simplified→traditional map.
Cantonese has no wordfreq table, so it borrows the 'zh' score for its shared Han spellings (粵字 like 冇
stay NULL → serving baseline). If wordfreq isn't installed, we fall back to the bundled rank lists.

freq is a score in (0,1]: a very common word ≈ 1.0, a rare one ≈ 0.15 (Zipf-scaled). NULL → baseline.
"""
import math

from ..db import SOURCES_DIR

NMAX = 50000
FLOOR = 0.15  # score for the least-frequent ranked word (still > the serving baseline for unmatched)
ZMAX = 7.0    # Zipf of an extremely common word; Zipf >= this maps to ~1.0

try:
    from wordfreq import zipf_frequency

    _HAVE_WORDFREQ = True
except Exception:  # pragma: no cover - exercised only when the dep is missing
    _HAVE_WORDFREQ = False


def _zipf_to_score(z: float) -> float | None:
    """Map a wordfreq Zipf value (≈0–8, higher = commoner) to our (0,1] score; 0 (unknown) → None."""
    if z <= 0:
        return None
    return round(FLOOR + (1.0 - FLOOR) * min(z, ZMAX) / ZMAX, 4)


def _load_ranked(path) -> dict[str, float]:
    """Fallback: word -> score, from a 'word count' per-line list (already frequency-ordered)."""
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
    """char-level simplified→traditional (fallback path only)."""
    m: dict[str, str] = {}
    for child, parent in conn.execute(
        "SELECT child_cp, parent_cp FROM glyph_edge WHERE type='simplification'"
    ):
        m.setdefault(chr(child), chr(parent))
    return m


def ingest(conn) -> None:
    rows = conn.execute("SELECT id, variety, reading FROM lexeme").fetchall()
    forms_by_lex: dict[int, list[str]] = {}
    for lex, form in conn.execute("SELECT lexeme_id, form FROM surface_form"):
        forms_by_lex.setdefault(lex, []).append(form)

    updates = []

    if _HAVE_WORDFREQ:
        # wordfreq lang per variety; Cantonese borrows Mandarin ('zh') for shared Han spellings.
        wf_lang = {"zh": "zh", "yue": "zh", "ja": "ja"}
        cache: dict[tuple[str, str], float | None] = {}

        def score_word(lang: str, word: str) -> float | None:
            key = (lang, word)
            if key not in cache:
                cache[key] = _zipf_to_score(zipf_frequency(word, lang))
            return cache[key]

        for lex, variety, reading in rows:
            lang = wf_lang.get(variety)
            if not lang:
                continue
            keys = list(forms_by_lex.get(lex, []))
            if variety == "ja" and reading:
                keys.append(reading)  # match a kana-only / kana-read word by its reading
            best: float | None = None
            for k in keys:
                s = score_word(lang, k)
                if s is not None and (best is None or s > best):
                    best = s
            if best is not None:
                updates.append((best, lex))
        conn.executemany("UPDATE lexeme SET freq=?1 WHERE id=?2", updates)
        print(f"    frequency: {len(updates)}/{len(rows)} lexemes scored via wordfreq")
        return

    # ---- fallback: the bundled OpenSubtitles rank lists ----
    zh = _load_ranked(SOURCES_DIR / "freq_zh_cn.txt")
    ja = _load_ranked(SOURCES_DIR / "freq_ja.txt")
    if not zh and not ja:
        print("    (no wordfreq and no frequency lists found - skipping)")
        return
    s2t = _simp_to_trad(conn)
    zh_wide = dict(zh)
    for word, score in zh.items():
        trad = "".join(s2t.get(c, c) for c in word)
        if trad != word:
            zh_wide.setdefault(trad, score)
    for lex, variety, reading in rows:
        m = ja if variety == "ja" else zh_wide
        keys = list(forms_by_lex.get(lex, [])) + ([reading] if (variety == "ja" and reading) else [])
        best = None
        for k in keys:
            if k in m and (best is None or m[k] > best):
                best = m[k]
        if best is not None:
            updates.append((best, lex))
    conn.executemany("UPDATE lexeme SET freq=?1 WHERE id=?2", updates)
    print(f"    frequency: {len(updates)}/{len(rows)} lexemes scored (fallback lists)")
