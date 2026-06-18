"""Item 18: CC-CEDICT `trad|simp[pinyin]` cross-reference markup must not leak into glosses."""
import pytest

from kogupipe.ingest.lexemes import clean_gloss


# 1. The canonical full form `trad|simp[pinyin]` collapses to the traditional headword.
def test_full_reference_collapses_to_traditional():
    assert clean_gloss(
        "numeral 1 in the Suzhou numeral system 蘇州碼子|苏州码子[Su1 zhou1 ma3 zi5]"
    ) == "numeral 1 in the Suzhou numeral system 蘇州碼子"


# 2. A `word[pinyin]` reference with no simplified twin drops the pinyin bracket.
def test_word_with_pinyin_drops_bracket():
    assert clean_gloss("abbr. for 萬|万[wan4]") == "abbr. for 萬"
    assert clean_gloss("see 開金|开金[kai1 jin1]") == "see 開金"


# 3. Multiple references in one gloss are all cleaned.
def test_multiple_references():
    assert clean_gloss(
        "Gen Z (abbr. for 95後|95后[jiu3 wu3 hou4] + 00後|00后[ling2 ling2 hou4])"
    ) == "Gen Z (abbr. for 95後 + 00後)"


# 4. A real pronunciation note in brackets (NOT glued to a preceding word) is preserved.
def test_pronunciation_note_preserved():
    g = "(Tw) to cram (Tai-lo pr. [khè-su], similar to 啃書|啃书[ken3 shu1])"
    assert clean_gloss(g) == "(Tw) to cram (Tai-lo pr. [khè-su], similar to 啃書)"


# 5. A plain English gloss with no markup is unchanged (idempotent / no false edits).
def test_plain_gloss_unchanged():
    for g in ["company; firm; corporation", "to learn; to study", "mountain"]:
        assert clean_gloss(g) == g


# 6. Cleaning is idempotent: running it on already-clean output is a no-op.
def test_idempotent():
    once = clean_gloss("abbr. for B型超聲|B型超声[B xing2 chao1 sheng1]")
    assert clean_gloss(once) == once == "abbr. for B型超聲"


# 7. The shipped DB must not contain leaked `X|Y[...]` markup after the build applies the cleaner.
def test_built_db_has_no_leaked_markup(db):
    n = db.execute(
        r"SELECT COUNT(*) FROM sense WHERE gloss_en LIKE '%|%[%]%'"
    ).fetchone()[0]
    assert n == 0, f"{n} glosses still carry trad|simp[pinyin] markup"
