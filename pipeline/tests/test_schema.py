"""Schema tests (Phase 0.3). >=5 cases + edge cases.

Run: cd pipeline && .venv/bin/pytest tests/test_schema.py -q
"""
import sqlite3

import pytest

from kanzipipe.db import create_db


@pytest.fixture()
def conn(tmp_path):
    c = create_db(tmp_path / "t.sqlite")
    yield c
    c.close()


# 1. The schema applies cleanly and creates all expected tables.
def test_schema_applies_and_has_core_tables(conn):
    tables = {r[0] for r in conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table'")}
    for t in ("region", "reform", "character", "char_reading", "glyph_edge",
              "lexeme", "surface_form", "sense", "concept", "sense_concept",
              "origin_badge", "etymology", "build_meta"):
        assert t in tables, f"missing table {t}"


# 2. Core-four regions are seeded and all in the launch set.
def test_core_four_regions_seeded(conn):
    rows = conn.execute(
        "SELECT code FROM region WHERE launch=1 ORDER BY code").fetchall()
    assert [r[0] for r in rows] == ["CN", "HK", "JP", "TW"]


# 3. Reform events needed by Phase 1.1 are present.
def test_reforms_seeded(conn):
    ids = {r[0] for r in conn.execute("SELECT id FROM reform")}
    assert {"prc-1964", "jp-toyo", "opencc"} <= ids


# 4. Foreign keys are enforced (orphan glyph_edge rejected).
def test_foreign_keys_enforced(conn):
    conn.execute("INSERT INTO character(cp,char,is_orthodox) VALUES (23398,'學',1)")
    with pytest.raises(sqlite3.IntegrityError):
        # parent_cp 99999999 does not exist
        conn.execute(
            "INSERT INTO glyph_edge(child_cp,parent_cp,type) VALUES (23398,99999999,'z-variant')")


# 5. FTS5 gloss_fts virtual table exists and is queryable against sense content.
def test_fts5_search(conn):
    conn.execute("INSERT INTO lexeme(id,variety,headword) VALUES (1,'zh','機場')")
    conn.execute("INSERT INTO sense(id,lexeme_id,gloss_en) VALUES (1,1,'airport; airfield')")
    conn.execute("INSERT INTO gloss_fts(rowid,gloss_en) VALUES (1,'airport; airfield')")
    rows = conn.execute(
        "SELECT rowid FROM gloss_fts WHERE gloss_fts MATCH 'airport'").fetchall()
    assert rows == [(1,)]


# --- edge cases ---

# E1. character.char is UNIQUE -> duplicate glyph rejected.
def test_duplicate_char_rejected(conn):
    conn.execute("INSERT INTO character(cp,char) VALUES (23398,'學')")
    with pytest.raises(sqlite3.IntegrityError):
        conn.execute("INSERT INTO character(cp,char) VALUES (99999,'學')")


# E2. WITHOUT ROWID composite PK on glyph_edge rejects exact duplicates but allows
#     the SAME child->parent under a DIFFERENT reform type (the 学 dual-edge case).
def test_glyph_edge_dual_reform_allowed(conn):
    for cp, ch in [(23398, "學"), (23398 + 1, "X"), (24037, "学")]:
        try:
            conn.execute("INSERT INTO character(cp,char) VALUES (?,?)", (cp, ch))
        except sqlite3.IntegrityError:
            pass
    # 学 (24037) -> 學 (23398) via BOTH simplification and shinjitai: two rows, allowed.
    conn.execute("INSERT INTO glyph_edge(child_cp,parent_cp,type,reform_id) VALUES (24037,23398,'simplification','prc-1964')")
    conn.execute("INSERT INTO glyph_edge(child_cp,parent_cp,type,reform_id) VALUES (24037,23398,'shinjitai','jp-toyo')")
    n = conn.execute("SELECT count(*) FROM glyph_edge WHERE child_cp=24037").fetchone()[0]
    assert n == 2
    # exact duplicate (same type) rejected
    with pytest.raises(sqlite3.IntegrityError):
        conn.execute("INSERT INTO glyph_edge(child_cp,parent_cp,type) VALUES (24037,23398,'simplification')")


# E3. region.folds_into self-reference works (Macau->HK style), enforced as FK.
def test_region_folds_into_fk(conn):
    conn.execute("INSERT INTO region(code,name,script,folds_into,launch) VALUES ('MO','Macau','trad','HK',0)")
    assert conn.execute("SELECT folds_into FROM region WHERE code='MO'").fetchone()[0] == "HK"
    with pytest.raises(sqlite3.IntegrityError):
        conn.execute("INSERT INTO region(code,name,script,folds_into) VALUES ('ZZ','Nowhere','simp','XX')")
