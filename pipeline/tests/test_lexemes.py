"""Phase 1.2 lexeme regression probes + edge cases. Uses the shared built DB (conftest.py)."""
import pytest


@pytest.fixture()
def conn(db):
    return db


def lexemes_for_form(conn, form):
    return conn.execute("""
        SELECT l.id, l.variety, l.headword, l.reading
        FROM surface_form sf JOIN lexeme l ON l.id = sf.lexeme_id
        WHERE sf.form = ?""", (form,)).fetchall()


# 1. Chinese is one lexeme with two skins: 機場/机场 is a single zh lexeme carrying both forms.
def test_chinese_one_lexeme_two_skins(conn):
    rows = lexemes_for_form(conn, "機場")
    zh = [r for r in rows if r[1] == "zh"]
    assert len(zh) == 1, "機場 should be exactly one zh lexeme"
    lid = zh[0][0]
    forms = {(f, s) for f, s in conn.execute(
        "SELECT form, script FROM surface_form WHERE lexeme_id=?", (lid,))}
    assert ("機場", "trad") in forms
    assert ("机场", "simp") in forms
    assert zh[0][3] == "ji1 chang3"


# 2. Japanese 会社 is its OWN lexeme with gloss 'company' - never merged into a Chinese one.
def test_japanese_separate_lexeme(conn):
    ja = [r for r in lexemes_for_form(conn, "会社") if r[1] == "ja"]
    assert len(ja) >= 1
    lid = ja[0][0]
    glosses = " ".join(g[0] for g in conn.execute(
        "SELECT gloss_en FROM sense WHERE lexeme_id=?", (lid,)))
    assert "company" in glosses.lower()
    # there must be no zh lexeme whose headword is 会社 (the simplified Japanese form merged in)
    assert conn.execute(
        "SELECT COUNT(*) FROM lexeme WHERE variety='zh' AND headword='会社'").fetchone()[0] == 0


# 3. The xx5 placeholder is suppressed: 々 is kept as a word but carries no reading, and NO
#    reading anywhere in the DB looks like the xx5 placeholder.
def test_no_xx5_leak(conn):
    row = conn.execute(
        "SELECT id, reading FROM lexeme WHERE variety='zh' AND headword='々'").fetchone()
    assert row is not None, "々 should be kept as a lexeme"
    assert row[1] is None, "々 reading should be suppressed (was xx5)"
    leaks = conn.execute(
        "SELECT COUNT(*) FROM lexeme_reading WHERE value GLOB '*[xX][xX][0-9]*'").fetchone()[0]
    assert leaks == 0


# 4. Pinyin is normalised for tolerant matching (numbered + toneless forms stored).
def test_pinyin_normalised_forms(conn):
    lid = [r for r in lexemes_for_form(conn, "機場") if r[1] == "zh"][0][0]
    kinds = {(k, v) for k, v in conn.execute(
        "SELECT kind, value FROM lexeme_reading WHERE lexeme_id=?", (lid,))}
    assert ("pinyin_num", "ji1chang3") in kinds
    assert ("pinyin_plain", "jichang") in kinds


# 5. FTS English search finds the airport sense.
def test_fts_english_search(conn):
    rows = conn.execute("""
        SELECT l.headword FROM gloss_fts
        JOIN sense s ON s.id = gloss_fts.rowid
        JOIN lexeme l ON l.id = s.lexeme_id
        WHERE gloss_fts MATCH 'airport' AND l.variety='zh'""").fetchall()
    assert any(h == "機場" for (h,) in rows)


# --- edge cases ---

# E1. A kana-only Japanese word has a primary kana surface form (no kanji to be primary).
def test_kana_only_primary(conn):
    # ござ-style kana-only entries exist; check the invariant holds wherever it applies
    bad = conn.execute("""
        SELECT l.id FROM lexeme l
        WHERE l.variety='ja'
          AND NOT EXISTS (SELECT 1 FROM surface_form sf WHERE sf.lexeme_id=l.id AND sf.script='shinjitai')
          AND NOT EXISTS (SELECT 1 FROM surface_form sf WHERE sf.lexeme_id=l.id AND sf.is_primary=1)
        LIMIT 1""").fetchone()
    assert bad is None, "every kana-only ja lexeme should have a primary kana form"


# E2. The 发←發/髮 merge surfaces at the lexeme level: 头发 (hair) is a simp form whose lexeme's
#     traditional skin is 頭髮.
def test_merge_lexeme_surface(conn):
    rows = lexemes_for_form(conn, "头发")
    zh = [r for r in rows if r[1] == "zh"]
    assert zh, "头发 should resolve to a zh lexeme"
    lid = zh[0][0]
    trad = {f for (f,) in conn.execute(
        "SELECT form FROM surface_form WHERE lexeme_id=? AND script='trad'", (lid,))}
    assert "頭髮" in trad


# E3. 會社 (guild) exists as a Chinese lexeme distinct from Japanese 会社 (company) - the raw
#     material for the false-friend label in Phase 2. They must NOT be the same lexeme.
def test_false_friend_material_distinct(conn):
    zh = conn.execute(
        "SELECT id FROM lexeme WHERE variety='zh' AND headword='會社'").fetchall()
    ja = [r for r in lexemes_for_form(conn, "会社") if r[1] == "ja"]
    assert zh, "會社 should exist as a zh lexeme"
    assert ja, "会社 should exist as a ja lexeme"
    assert {r[0] for r in zh}.isdisjoint({r[0] for r in ja})
