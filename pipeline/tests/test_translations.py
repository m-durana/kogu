"""Wiktionary cross-language bridge probes (ingest/translations.py).

Builds a tiny self-contained DB (a handful of lexemes/forms) and drives translations.ingest over a
small inline TSV fixture, so the unit tests don't depend on the ~20GB kaikki dump being acquired.
The final test is an INTEGRATION check against the real session-built DB, gated on the compact
translations source actually being present.
"""
import sqlite3
from pathlib import Path

import pytest

from kogupipe.ingest import translations

# --- minimal fixture DB (only the tables the resolver + ingest touch) -------------------------

_MINI_SCHEMA = """
CREATE TABLE lexeme (
    id INTEGER PRIMARY KEY, variety TEXT NOT NULL, headword TEXT NOT NULL,
    reading TEXT, freq REAL, freq_source TEXT);
CREATE TABLE surface_form (
    id INTEGER PRIMARY KEY, lexeme_id INTEGER NOT NULL, form TEXT NOT NULL,
    script TEXT NOT NULL, region TEXT, is_primary INTEGER NOT NULL DEFAULT 0);
CREATE TABLE sense (
    id INTEGER PRIMARY KEY, lexeme_id INTEGER NOT NULL, pos TEXT,
    gloss_en TEXT NOT NULL, sense_order INTEGER NOT NULL DEFAULT 0);
"""

# (id, variety, headword)
_LEXEMES = [
    (1, "zh", "攀岩"),       # rock climbing (zh)
    (2, "ja", "クライミング"),  # climbing (ja, katakana) - written differently from 攀岩
    (3, "yue", "攀石"),       # rock climbing (yue) - written differently again
    (4, "zh", "自由"),       # freedom (zh)
    (5, "ja", "自由"),       # freedom (ja) - SAME glyphs as the zh form
    (6, "zh", "攀缘"),       # climb/clamber (zh synonym) - same variety as 攀岩
]


def _mini_db():
    conn = sqlite3.connect(":memory:")
    conn.executescript(_MINI_SCHEMA)
    for lid, var, hw in _LEXEMES:
        conn.execute("INSERT INTO lexeme(id,variety,headword) VALUES (?,?,?)", (lid, var, hw))
        conn.execute(
            "INSERT INTO surface_form(lexeme_id,form,script,is_primary) VALUES (?,?,?,1)",
            (lid, hw, "trad"))
        conn.execute(
            "INSERT INTO sense(lexeme_id,gloss_en,sense_order) VALUES (?,?,0)", (lid, hw))
    conn.commit()
    return conn


def _write_tsv(tmp_path: Path, rows) -> Path:
    p = tmp_path / "wt.tsv"
    p.write_text("\n".join("\t".join(r) for r in rows) + "\n", encoding="utf-8")
    return p


def _edges(conn):
    return set(conn.execute(
        "SELECT src_lexeme_id, dst_lexeme_id, relation, source FROM lexeme_equivalent").fetchall())


# 1. A translation row group resolves to cross-language edges (both directions, all pairs).
def test_resolves_to_edges(tmp_path):
    conn = _mini_db()
    tsv = _write_tsv(tmp_path, [
        ("rock climbing", "cmn", "攀岩"),
        ("rock climbing", "ja", "クライミング"),
        ("rock climbing", "yue", "攀石"),
    ])
    translations.ingest(conn, tsv)
    e = _edges(conn)
    # every ordered pair of the 3 distinct cross-variety members => 6 directed edges
    assert (1, 2, "cross-lang", "wiktionary") in e
    assert (2, 1, "cross-lang", "wiktionary") in e
    assert (1, 3, "cross-lang", "wiktionary") in e
    assert (3, 2, "cross-lang", "wiktionary") in e
    assert len(e) == 6, e


# 2. Same-spelling pair (zh 自由 / ja 自由) is skipped - not "written differently".
def test_same_spelling_skipped(tmp_path):
    conn = _mini_db()
    tsv = _write_tsv(tmp_path, [
        ("freedom", "zh", "自由"),
        ("freedom", "ja", "自由"),
    ])
    translations.ingest(conn, tsv)
    assert _edges(conn) == set()


# 2b. Same-variety synonyms (zh 攀岩 / zh 攀缘) never weld into an edge.
def test_same_variety_skipped(tmp_path):
    conn = _mini_db()
    tsv = _write_tsv(tmp_path, [
        ("clamber", "zh", "攀岩"),
        ("clamber", "cmn", "攀缘"),
    ])
    translations.ingest(conn, tsv)
    assert _edges(conn) == set()


# 3. Unresolved target words are skipped (and never invent a link).
def test_unresolved_skipped(tmp_path):
    conn = _mini_db()
    tsv = _write_tsv(tmp_path, [
        ("rock climbing", "cmn", "攀岩"),
        ("rock climbing", "ja", "存在しない語"),  # not in DB
    ])
    translations.ingest(conn, tsv)
    # 攀岩 resolves but its only partner is unresolved -> no edge
    assert _edges(conn) == set()


# 4. Idempotent: re-running yields the same edge set and only manages source='wiktionary'.
def test_idempotent_and_source_scoped(tmp_path):
    conn = _mini_db()
    # pre-seed a curated edge that the wiktionary step must NOT touch
    conn.execute(
        "CREATE TABLE IF NOT EXISTS lexeme_equivalent (src_lexeme_id INTEGER NOT NULL,"
        " dst_lexeme_id INTEGER NOT NULL, relation TEXT NOT NULL, source TEXT NOT NULL,"
        " PRIMARY KEY (src_lexeme_id,dst_lexeme_id,relation)) WITHOUT ROWID")
    conn.execute("INSERT INTO lexeme_equivalent VALUES (1,3,'cross-lang','curated')")
    tsv = _write_tsv(tmp_path, [
        ("rock climbing", "cmn", "攀岩"),
        ("rock climbing", "ja", "クライミング"),
    ])
    translations.ingest(conn, tsv)
    first = _edges(conn)
    translations.ingest(conn, tsv)
    second = _edges(conn)
    assert first == second
    assert (1, 3, "cross-lang", "curated") in second  # curated survived
    assert (1, 2, "cross-lang", "wiktionary") in second


# 5. Absent source file is a graceful no-op (build never crashes when data deferred).
def test_missing_source_noop(tmp_path):
    conn = _mini_db()
    conn.execute(
        "CREATE TABLE IF NOT EXISTS lexeme_equivalent (src_lexeme_id INTEGER NOT NULL,"
        " dst_lexeme_id INTEGER NOT NULL, relation TEXT NOT NULL, source TEXT NOT NULL,"
        " PRIMARY KEY (src_lexeme_id,dst_lexeme_id,relation)) WITHOUT ROWID")
    translations.ingest(conn, tmp_path / "does_not_exist.tsv")
    assert _edges(conn) == set()


# 6. INTEGRATION (gated): after the full build, if the compact translations source is present,
#    the Wiktionary step must have produced real cross-language bridges: at least one zh lexeme
#    gains a ja- or yue-written equivalent (the 攀岩-style bridge the feature is for). The exact
#    headword present depends on which entries Wiktionary actually translates, so we assert the
#    MECHANISM over the real data rather than one hand-picked word.
def test_integration_crosslang_bridges(db):
    from kogupipe.ingest.translations import _DEFAULT
    if not _DEFAULT.exists():
        pytest.skip("wiktionary_translations.tsv not acquired - data download deferred")
    conn = db
    n = conn.execute(
        "SELECT COUNT(*) FROM lexeme_equivalent WHERE source='wiktionary'").fetchone()[0]
    assert n > 0, "no wiktionary edges produced from the acquired data"
    # a zh lexeme bridged to a ja or yue lexeme written differently, via wiktionary
    zh_to_jayue = conn.execute(
        "SELECT COUNT(*) FROM lexeme_equivalent le "
        "JOIN lexeme a ON a.id=le.src_lexeme_id JOIN lexeme b ON b.id=le.dst_lexeme_id "
        "WHERE le.source='wiktionary' AND a.variety='zh' AND b.variety IN ('ja','yue') "
        "AND a.headword<>b.headword").fetchone()[0]
    assert zh_to_jayue > 0, "no zh->ja/yue wiktionary bridges"
    # and the guards held even on real data: no same-variety / same-form wiktionary edges
    bad = conn.execute(
        "SELECT COUNT(*) FROM lexeme_equivalent le "
        "JOIN lexeme a ON a.id=le.src_lexeme_id JOIN lexeme b ON b.id=le.dst_lexeme_id "
        "WHERE le.source='wiktionary' AND (a.variety=b.variety OR a.headword=b.headword)"
    ).fetchone()[0]
    assert bad == 0, f"{bad} bad wiktionary edges (same variety or same form)"
