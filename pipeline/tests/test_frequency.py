"""Frequency ingest (wordfreq-backed): score mapping + coverage/ranking on the built DB."""
from kogupipe.ingest.frequency import _zipf_to_score, FLOOR


# 1. Zipf 0 (word unknown to wordfreq) → None (left NULL, serving layer applies a baseline).
def test_unknown_word_is_none():
    assert _zipf_to_score(0) is None
    assert _zipf_to_score(-1) is None


# 2. A very common word (high Zipf) maps to ~1.0 (capped at ZMAX).
def test_common_word_near_one():
    assert _zipf_to_score(7.5) == 1.0
    assert _zipf_to_score(8.0) == 1.0  # clamped


# 3. Monotonic: a commoner word scores higher than a rarer one.
def test_monotonic():
    assert _zipf_to_score(6.0) > _zipf_to_score(4.0) > _zipf_to_score(2.0)


# 4. All scores stay within (FLOOR, 1].
def test_score_range():
    for z in (0.1, 1, 3, 5, 7):
        s = _zipf_to_score(z)
        assert FLOOR <= s <= 1.0


# 5. Built DB: Mandarin frequency coverage is broad (was ~23% with the old 50k list).
def test_zh_coverage_is_broad(db):
    total, scored = db.execute(
        "SELECT count(*), sum(CASE WHEN freq IS NOT NULL THEN 1 ELSE 0 END) FROM lexeme WHERE variety='zh'"
    ).fetchone()
    assert scored / total > 0.7, f"zh coverage only {scored}/{total}"


# 6. Built DB: a common word outranks a rare one in the same language.
def test_common_outranks_rare(db):
    common = db.execute("SELECT freq FROM lexeme WHERE headword='人' AND variety='zh' LIMIT 1").fetchone()
    rare = db.execute("SELECT freq FROM lexeme WHERE headword='淼' AND variety='zh' LIMIT 1").fetchone()
    assert common and common[0] is not None
    if rare and rare[0] is not None:
        assert common[0] > rare[0]
