"""Loanword-origin probes (JMdict <lsource> → etymology gap-fill + badges).

Pure-function tests for the deterministic templating (no LLM), plus a built-DB check that a gairaigo
with no Wiktionary etymology gets the JMdict-sourced origin and a katakana entry's source isn't
overwritten when Wiktionary already has one.
"""
import pytest

from kogupipe.ingest.loanwords import loan_badges, loan_text


def test_single_source_with_spelling():
    assert loan_text([{"lang": "fre", "text": "art déco", "wasei": False}]) == "From French “art déco”."


def test_multiple_languages_joined():
    txt = loan_text(
        [{"lang": "eng", "text": "ice", "wasei": False}, {"lang": "ger", "text": "Eis", "wasei": False}]
    )
    assert txt == "From English “ice” and German “Eis”."


def test_wasei_is_japanese_coinage():
    assert loan_text([{"lang": "eng", "text": "baby car", "wasei": True}]) == "Japanese coinage from English “baby car”."


def test_source_without_spelling():
    assert loan_text([{"lang": "kor", "text": None, "wasei": False}]) == "From Korean."


def test_badges_wasei_eigo_and_language():
    assert loan_badges([{"lang": "eng", "wasei": True}]) == {"borrowed-from-english", "wasei-eigo"}
    assert loan_badges([{"lang": "fre", "wasei": False}]) == {"borrowed-from-french"}


def test_built_db_gairaigo_gap_filled(db):
    # アールデコ has no Wiktionary etymology → JMdict lsource fills a French origin.
    rows = db.execute(
        "SELECT e.text, e.source FROM lexeme l JOIN etymology e ON e.lexeme_id = l.id "
        "WHERE l.variety='ja' AND l.headword='アールデコ'"
    ).fetchall()
    assert rows, "expected an etymology row for アールデコ"
    assert any("French" in t and s == "jmdict" for t, s in rows), rows


def test_built_db_does_not_overwrite_wiktionary(db):
    # アイス has a Wiktionary etymology; the JMdict gap-fill must not replace it.
    rows = db.execute(
        "SELECT source FROM lexeme l JOIN etymology e ON e.lexeme_id = l.id "
        "WHERE l.variety='ja' AND l.headword='アイス'"
    ).fetchall()
    # if アイス is present at all, its etymology stays the Wiktionary one (never jmdict-overwritten)
    if rows:
        assert all(s == "wiktionary" for (s,) in rows), rows
