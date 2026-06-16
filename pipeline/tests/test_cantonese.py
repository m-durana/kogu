"""Cantonese 粵字 retag probes (DESIGN.md §2.2).

CC-CEDICT absorbed Cantonese-only chars as zh entries with a nominal Mandarin reading; the retag
relabels them to 粵 with jyutping (and splits genuinely-mixed homographs). Uses the session-built DB.
"""
import shutil
import sqlite3

import pytest

from kogupipe.ingest.cantonese import _is_canto_sense, retag


@pytest.fixture()
def conn(db):
    return db


def varieties(conn, head):
    return sorted({v for (v,) in conn.execute("SELECT variety FROM lexeme WHERE headword=?", (head,))})


def readings(conn, head, variety):
    return {
        k
        for (k,) in conn.execute(
            "SELECT lr.kind FROM lexeme l JOIN lexeme_reading lr ON lr.lexeme_id=l.id "
            "WHERE l.headword=? AND l.variety=?",
            (head, variety),
        )
    }


# 1. Core 粵字 are Cantonese (粵), with jyutping and NO nominal Mandarin pinyin.
@pytest.mark.parametrize("ch", ["冇", "喺", "咗", "嘢", "啲", "佢"])
def test_yuezi_relabeled(conn, ch):
    assert varieties(conn, ch) == ["yue"], f"{ch} should be yue-only"
    kinds = readings(conn, ch, "yue")
    assert "jyutping" in kinds, f"{ch} missing jyutping"
    assert "pinyin" not in kinds, f"{ch} still carries nominal Mandarin pinyin"


# 2. Genuinely-mixed homographs keep BOTH 中 and 粵 (separate senses/lexemes).
@pytest.mark.parametrize("ch", ["乜", "嘅"])
def test_mixed_homograph(conn, ch):
    vs = varieties(conn, ch)
    assert "yue" in vs and "zh" in vs, f"{ch} should have both zh and yue: {vs}"
    assert "jyutping" in readings(conn, ch, "yue")


# 3. False positives — Mandarin words whose gloss only MENTIONS Cantonese (etymology/context) stay 中.
@pytest.mark.parametrize("ch", ["基", "夏娃", "曲奇", "點心", "唐人"])
def test_not_overtagged(conn, ch):
    assert "yue" not in varieties(conn, ch), f"{ch} was wrongly relabeled Cantonese"


# 4. Shared vocabulary still gets jyutping attached (regression of the readings-attach path).
def test_shared_vocab_keeps_jyutping(conn):
    assert "jyutping" in readings(conn, "學校", "zh")


# 5. The classifier: assert/exclude on the real gloss patterns.
def test_is_canto_sense():
    assert _is_canto_sense("to not have (Cantonese) (Mandarin equivalent: 沒有)")
    assert _is_canto_sense("Cantonese particle equivalent to 了")
    assert _is_canto_sense("possessive particle (Cantonese); Mandarin equivalent: 的")
    assert not _is_canto_sense("(bound form) gay (loanword from English into Cantonese)")
    assert not _is_canto_sense("cookie (loanword via Cantonese 曲奇)")
    assert not _is_canto_sense("dim sum (in Cantonese cooking)")
    assert not _is_canto_sense("(used in overseas Cantonese communities) Chinese person")


# 6. Idempotent: re-running the retag changes nothing.
def test_retag_idempotent(built_db, tmp_path):
    cp = tmp_path / "idem.sqlite"
    shutil.copy(built_db, cp)
    c = sqlite3.connect(cp)
    before = dict(c.execute("SELECT variety, COUNT(*) FROM lexeme GROUP BY variety").fetchall())
    retag(c)
    c.commit()
    after = dict(c.execute("SELECT variety, COUNT(*) FROM lexeme GROUP BY variety").fetchall())
    c.close()
    assert before == after  # build already ran retag → second run is a no-op
