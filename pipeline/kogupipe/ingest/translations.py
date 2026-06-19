"""Phase 2.3 - cross-language equivalence bridges from English Wiktionary translation tables.

English Wiktionary (via kaikki / wiktextract) lists, for each English headword, the word each
language uses for that meaning: "rock climbing" -> zh 攀岩, ja クライミング / 岩登り, yue 攀石.
Those are exactly the "same meaning, different word, different language" bridges the gloss-pivot
concept layer misses (it only joins identically-glossed senses), and the curated cross-lang map
covers by hand. This step layers the Wiktionary triples on top of the curated edges.

Source is a COMPACT translations file produced by streaming the (~20GB) kaikki English dump and
keeping only the zh/cmn/yue/ja targets - never the raw dump (see scripts/fetch_translations.py).
File format (TSV, header optional, '#'-comments ok):  en_headword <TAB> lang <TAB> word
where lang is one of zh|cmn|yue|ja (cmn is folded to zh). Multiple rows per headword are fine.

Every target word is resolved to an existing lexeme of the right variety via surface_form; an
unresolved word can never invent a link (it is skipped and counted). Among the resolved members
of one English headword we emit a lexeme_equivalent edge for every pair of DIFFERENT varieties
that is ALSO written DIFFERENTLY (reusing the equivalents guard - shared glyphs like zh/ja 自由
are not "written differently"). Edges carry source='wiktionary'; idempotent per source.
"""
from __future__ import annotations

import csv
from pathlib import Path

from .equivalents import _ensure_table, _resolver

_SOURCES = Path(__file__).resolve().parents[2] / "sources"
_DEFAULT = _SOURCES / "wiktionary_translations.tsv"

# wiktextract language codes -> Kogu variety. cmn (Mandarin) and the bare macro-code zh both map
# to Kogu's 'zh'; yue (Cantonese) and ja (Japanese) map directly.
_LANG_VARIETY = {"zh": "zh", "cmn": "zh", "yue": "yue", "ja": "ja"}

_RELATION = "cross-lang"
_SOURCE = "wiktionary"


def _grouped(path: Path):
    """Yield (en_headword, [(variety, word), ...]) groups from the compact TSV."""
    groups: dict[str, list[tuple[str, str]]] = {}
    order: list[str] = []
    with path.open(encoding="utf-8") as f:
        for raw in f:
            line = raw.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 3:
                continue
            en, lang, word = parts[0].strip(), parts[1].strip().lower(), parts[2].strip()
            variety = _LANG_VARIETY.get(lang)
            if not en or not variety or not word:
                continue
            if en not in groups:
                groups[en] = []
                order.append(en)
            groups[en].append((variety, word))
    for en in order:
        yield en, groups[en]


def ingest(conn, path: Path | None = None) -> None:
    _ensure_table(conn)
    # idempotent for THIS source only - leave inline/curated edges (run earlier) untouched.
    conn.execute("DELETE FROM lexeme_equivalent WHERE source = ?", (_SOURCE,))

    src = Path(path) if path is not None else _DEFAULT
    if not src.exists():
        print(f"      translations: source {src.name} absent - skipped (run fetch_translations)")
        return

    resolve = _resolver(conn)
    # the resolved lexeme's canonical headword, for the "written differently" guard. A Wiktionary
    # target string can be a variant/furigana spelling that resolves to a lexeme whose headword is
    # identical across varieties (zh 以色列 / ja 以色列) - comparing the source strings would miss
    # that, so we compare the resolved HEADWORDS (matching equivalents.py's same-glyph guard).
    head_cache: dict[int, str] = {}

    def headword(lid: int) -> str:
        if lid not in head_cache:
            r = conn.execute("SELECT headword FROM lexeme WHERE id = ?", (lid,)).fetchone()
            head_cache[lid] = r[0] if r else ""
        return head_cache[lid]

    edges: set[tuple[int, int, str, str]] = set()
    headwords = resolved_words = unresolved_words = 0

    for _en, targets in _grouped(src):
        headwords += 1
        members: list[tuple[int, str, str]] = []  # (lexeme_id, variety, resolved_headword)
        seen_ids: set[int] = set()
        for variety, word in targets:
            lid = resolve(word, variety)
            if lid is None:
                unresolved_words += 1
                continue
            resolved_words += 1
            if lid not in seen_ids:
                seen_ids.add(lid)
                hw = headword(lid)
                # Skip single-character members. An English-gloss pivot is far too polysemous for a
                # lone character (English "color" -> 色 but also 上色/顏色), so auto cross-language
                # bridges to/from single chars are noisy (色->上色, 違う->不, 神->上帝). The single
                # character's real cross-language word is surfaced instead by the concept- and
                # frequency-gated "everyday word" path (耳 -> 耳朵). Hand-verified single-char pairs
                # (信/手紙) live in the CURATED bridges, which are unaffected.
                if len(hw) < 2:
                    continue
                members.append((lid, variety, hw))
        for a_id, a_var, a_head in members:
            for b_id, b_var, b_head in members:
                # a real CROSS-LANGUAGE bridge only: a different variety AND written DIFFERENTLY
                # (skip shared glyphs zh/ja 自由) - the same guard as ingest/equivalents.py. The
                # different-variety test stops two zh synonyms from one English headword (攀岩 /
                # 攀缘) being welded into an equivalence edge.
                if a_id != b_id and a_var != b_var and a_head != b_head:
                    edges.add((a_id, b_id, _RELATION, _SOURCE))

    conn.executemany(
        "INSERT OR IGNORE INTO lexeme_equivalent(src_lexeme_id,dst_lexeme_id,relation,source) "
        "VALUES (?,?,?,?)",
        list(edges),
    )
    print(
        f"      translations: headwords={headwords} resolved-words={resolved_words} "
        f"(unresolved {unresolved_words}) edges={len(edges)}"
    )
