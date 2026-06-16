"""Explicit equivalence edge probes (ingest/equivalents.py).

The CC-Canto inline "Mandarin equivalent" notes and the curated 粵→中 / cross-language maps are
lifted into lexeme_equivalent. Uses the session-built DB (equivalents.ingest runs in the build).
"""
import sqlite3

import pytest


@pytest.fixture()
def conn(db):
    return db


def equivalents(conn, head, variety):
    """(variety, headword, relation) of every lexeme bridged to `head`/`variety`, both directions."""
    return {
        (v, hw, rel)
        for (v, hw, rel) in conn.execute(
            "SELECT l2.variety, l2.headword, le.relation FROM lexeme l1 "
            "JOIN lexeme_equivalent le ON le.src_lexeme_id = l1.id OR le.dst_lexeme_id = l1.id "
            "JOIN lexeme l2 ON l2.id = CASE WHEN le.src_lexeme_id = l1.id "
            "                               THEN le.dst_lexeme_id ELSE le.src_lexeme_id END "
            "WHERE l1.headword = ? AND l1.variety = ?",
            (head, variety),
        )
    }


# 1. The "mao": colloquial Cantonese 冇 bridges to standard Chinese 沒有 (the inline CC-Canto note).
def test_cantonese_to_standard(conn):
    eq = equivalents(conn, "冇", "yue")
    assert ("zh", "沒有", "colloquial-standard") in eq, eq


# 2. Curated cross-language bridge: zh 機場 ↔ ja 空港.
def test_crosslang_airport(conn):
    eq = equivalents(conn, "機場", "zh")
    assert ("ja", "空港", "cross-lang") in eq, eq


# 3. Resolver prefers the lexeme HEADED by the form, not one where it's a simplified alias:
#    屋企's "Mandarin equivalent: 家" must resolve to zh 家, never to 傢 (whose simp form is 家).
def test_resolver_prefers_headword(conn):
    eq = equivalents(conn, "屋企", "yue")
    zh = {hw for (v, hw, _) in eq if v == "zh"}
    assert "家" in zh and "傢" not in zh, eq


# 4. Data-quality invariant: an equivalence edge is "written differently" - never the same glyph.
def test_no_same_form_edges(conn):
    n = conn.execute(
        "SELECT COUNT(*) FROM lexeme_equivalent le "
        "JOIN lexeme a ON a.id = le.src_lexeme_id JOIN lexeme b ON b.id = le.dst_lexeme_id "
        "WHERE a.headword = b.headword"
    ).fetchone()[0]
    assert n == 0, f"{n} bogus same-form bridge(s)"


# 5. Every edge endpoint is a real lexeme of a different variety than nothing-resolves to junk.
def test_edges_resolve(conn):
    total = conn.execute("SELECT COUNT(*) FROM lexeme_equivalent").fetchone()[0]
    dangling = conn.execute(
        "SELECT COUNT(*) FROM lexeme_equivalent le "
        "LEFT JOIN lexeme a ON a.id = le.src_lexeme_id LEFT JOIN lexeme b ON b.id = le.dst_lexeme_id "
        "WHERE a.id IS NULL OR b.id IS NULL"
    ).fetchone()[0]
    assert total > 100 and dangling == 0, f"total={total} dangling={dangling}"
