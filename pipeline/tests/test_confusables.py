"""Confusable look-alike probes (Unihan kSpoofingVariant → char_confusable).

A visual-confusability signal only — must stay OUT of the variant graph (glyph_edge) and be stored
symmetrically.
"""


def test_table_populated(db):
    n = db.execute("SELECT count(*) FROM char_confusable").fetchone()[0]
    assert n > 50, f"expected confusable pairs, got {n}"


def test_pair_is_symmetric(db):
    # 滅 ↔ 㓕 is a kSpoofingVariant pair; both directions must exist.
    a, b = ord("滅"), ord("㓕")
    fwd = db.execute("SELECT 1 FROM char_confusable WHERE cp=? AND confusable_cp=?", (a, b)).fetchone()
    rev = db.execute("SELECT 1 FROM char_confusable WHERE cp=? AND confusable_cp=?", (b, a)).fetchone()
    assert fwd and rev, "confusable pair must be stored in both directions"


def test_no_self_pairs(db):
    n = db.execute("SELECT count(*) FROM char_confusable WHERE cp = confusable_cp").fetchone()[0]
    assert n == 0, "a character must not be its own confusable"


def test_endpoints_are_real_characters(db):
    # every endpoint references a real row in character (FK integrity / no dangling codepoints)
    bad = db.execute(
        "SELECT count(*) FROM char_confusable cc "
        "WHERE cc.cp NOT IN (SELECT cp FROM character) "
        "   OR cc.confusable_cp NOT IN (SELECT cp FROM character)"
    ).fetchone()[0]
    assert bad == 0, f"{bad} confusable endpoints are not characters"


def test_not_mixed_into_variant_graph(db):
    # the 滅/㓕 look-alike must NOT have leaked into glyph_edge (it is not a variant relationship)
    a, b = ord("滅"), ord("㓕")
    leaked = db.execute(
        "SELECT count(*) FROM glyph_edge WHERE (child_cp=? AND parent_cp=?) OR (child_cp=? AND parent_cp=?)",
        (a, b, b, a),
    ).fetchone()[0]
    assert leaked == 0, "spoofing confusable leaked into the variant graph"
