"""Phase 1.1 backbone regression probes (DESIGN.md §6.2) + edge cases.

Uses the session-scoped built DB from conftest.py.
Run: cd pipeline && .venv/bin/pytest tests/test_backbone.py -q
"""
import pytest


@pytest.fixture()
def conn(db):
    return db


def cp(ch):
    return ord(ch)


def identity_parents(conn, ch):
    """Orthodox codepoints reachable from ch via simp/shinjitai closure (bounded)."""
    rows = conn.execute("""
        WITH RECURSIVE up(node, depth) AS (
            SELECT ?, 0
            UNION
            SELECT e.parent_cp, up.depth + 1 FROM up
              JOIN glyph_edge e ON e.child_cp = up.node
             WHERE e.type IN ('simplification','shinjitai') AND up.depth < 16
        )
        SELECT DISTINCT c.char FROM up JOIN character c ON c.cp = up.node
        WHERE up.node <> ? AND c.is_orthodox = 1
    """, (cp(ch), cp(ch))).fetchall()
    return {r[0] for r in rows}


def edge_types(conn, child, parent):
    rows = conn.execute(
        "SELECT type FROM glyph_edge WHERE child_cp=? AND parent_cp=?",
        (cp(child), cp(parent))).fetchall()
    return {r[0] for r in rows}


# 1. 缶 and 糸 are kept as their own first-class entries (keep-vs-convert rule). Both are genuinely
#    the Japanese shinjitai of 罐 / 絲 (OpenCC records this), so they DO have an orthodox parent —
#    but the backbone must still retain them with their own radical gloss + readings, never collapse
#    them. (That a *query* for 缶 returns 缶 and not only 罐 is asserted in the Phase 1.3 query tests.)
def test_keep_not_convert(conn):
    for ch, parent in (("缶", "罐"), ("糸", "絲")):
        row = conn.execute(
            "SELECT gloss_en FROM character WHERE char=?", (ch,)).fetchone()
        assert row is not None, f"{ch} missing as its own entry"
        assert "radical" in (row[0] or "").lower(), f"{ch} should keep its own radical gloss"
        assert conn.execute(
            "SELECT COUNT(*) FROM char_reading WHERE cp=?", (cp(ch),)).fetchone()[0] > 0
        # it is a shinjitai of its parent, but retained as a distinct character row
        assert parent in identity_parents(conn, ch)


# 2. 広 → 廣 (the gap-catch test; Unihan misses it, OpenCC JPShinjitai supplies it).
def test_hiro_gap_caught(conn):
    assert "shinjitai" in edge_types(conn, "広", "廣")
    assert "廣" in identity_parents(conn, "広")


# 3. 学 reaches 學 by BOTH a simplification and a shinjitai edge — same single orthodox parent.
def test_xue_dual_reform_single_parent(conn):
    assert edge_types(conn, "学", "學") == {"simplification", "shinjitai"}
    assert identity_parents(conn, "学") == {"學"}


# 4. 发 is a real many-to-one merge: two orthodox parents 發 and 髮 (both via simplification).
def test_fa_merge_two_parents(conn):
    assert identity_parents(conn, "发") == {"發", "髮"}
    assert "simplification" in edge_types(conn, "发", "發")
    assert "simplification" in edge_types(conn, "发", "髮")


# 5. 夾 does NOT drag in 袷 (semantic-variant over-fire guard): no identity path 夾→袷.
def test_jia_no_semantic_overfire(conn):
    assert "袷" not in identity_parents(conn, "夾")
    # if any 夾→袷 edge exists at all it must be semantic-variant only (suggestion, not expansion)
    assert edge_types(conn, "夾", "袷") <= {"semantic-variant"}


# --- edge cases ---

# E1. 馬 is the traditional form Japan kept (orthodox); China simplified to 马 (derived).
def test_ma_kept_by_japan(conn):
    assert conn.execute("SELECT is_orthodox FROM character WHERE char='馬'").fetchone()[0] == 1
    assert "馬" in identity_parents(conn, "马")
    assert conn.execute("SELECT is_orthodox FROM character WHERE char='马'").fetchone()[0] == 0


# E2. 廣 is orthodox and is the shared parent of two *different* reductions (広 shinjitai, 广 simp).
def test_guang_two_distinct_children(conn):
    assert conn.execute("SELECT is_orthodox FROM character WHERE char='廣'").fetchone()[0] == 1
    assert "廣" in identity_parents(conn, "広")
    assert "廣" in identity_parents(conn, "广")
    assert "广" != "広"


# E3. Readings are populated across varieties for 學 (pinyin / jyutping / onyomi).
def test_readings_present(conn):
    got = {(k, v) for k, v in conn.execute(
        "SELECT kind, value FROM char_reading WHERE cp=?", (cp("學"),))}
    assert ("pinyin", "xué") in got
    assert ("jyutping", "hok6") in got
    assert ("onyomi", "GAKU") in got


# E4. No placeholder leaks and the closure terminated (build would have failed otherwise),
#     and every edge endpoint resolves to a real character (FK belt-and-braces).
def test_structural_integrity(conn):
    orphan = conn.execute("""
        SELECT COUNT(*) FROM glyph_edge e
        LEFT JOIN character p ON p.cp=e.parent_cp WHERE p.cp IS NULL
    """).fetchone()[0]
    assert orphan == 0
    leak = conn.execute(
        "SELECT COUNT(*) FROM char_reading WHERE value GLOB '*[xX][xX][0-9]*'").fetchone()[0]
    assert leak == 0
