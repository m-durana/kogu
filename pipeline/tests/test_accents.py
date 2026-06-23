"""Kanjium pitch-accent ingest tests: parse rules, the classic minimal pairs, and idempotency.

Builds a small fixture DB from schema.sql (like the other pipeline tests) and runs the real ingest
against a tiny in-test accents.txt, so no large download or live DB is touched.

Run: cd pipeline && .venv/bin/pytest tests/test_accents.py -q
"""
import pytest

from kogupipe.db import create_db
from kogupipe.ingest import accents


# A tiny accents.txt covering the minimal pairs + a multi-accent word + a kana-only word + a POS tag.
FIXTURE = "\n".join([
    "箸\tはし\t1",      # atamadaka
    "橋\tはし\t2",      # odaka
    "端\tはし\t0",      # heiban
    "雨\tあめ\t1",      # atamadaka
    "飴\tあめ\t0",      # heiban
    "寿司\tすし\t2,1",  # multi-accent: keep the list, serve the first
    "ああいう\t\t0",     # kana-only word (kana field empty)
    "二つ\tふたつ\t(副)0,(名)3",  # POS-tagged cell → "0,3"
])


def _seed_ja_word(conn, lid, kanji, kana, script="shinjitai"):
    """A ja lexeme with one kanji surface form, one kana surface form, and a kana reading."""
    conn.execute("INSERT INTO lexeme(id,variety,headword,reading) VALUES (?,?,?,?)", (lid, "ja", kanji, kana))
    conn.execute("INSERT INTO surface_form(lexeme_id,form,script,is_primary) VALUES (?,?,?,1)", (lid, kanji, script))
    conn.execute("INSERT INTO surface_form(lexeme_id,form,script) VALUES (?,?, 'kana')", (lid, kana))
    conn.execute("INSERT INTO lexeme_reading(lexeme_id,kind,value) VALUES (?, 'kana', ?)", (lid, kana))


def _seed_kana_only(conn, lid, kana):
    """A kana-only ja word: no kanji surface form, just the kana."""
    conn.execute("INSERT INTO lexeme(id,variety,headword,reading) VALUES (?,?,?,?)", (lid, "ja", kana, kana))
    conn.execute("INSERT INTO surface_form(lexeme_id,form,script,is_primary) VALUES (?,?, 'kana', 1)", (lid, kana))
    conn.execute("INSERT INTO lexeme_reading(lexeme_id,kind,value) VALUES (?, 'kana', ?)", (lid, kana))


@pytest.fixture()
def conn(tmp_path, monkeypatch):
    c = create_db(tmp_path / "t.sqlite")
    # write the fixture accents.txt and point the ingest module's SRC at it
    src = tmp_path / "accents.txt"
    src.write_text(FIXTURE, encoding="utf-8")
    monkeypatch.setattr(accents, "SRC", src)
    # the homograph minimal pairs (each kanji disambiguates the shared kana はし / あめ)
    _seed_ja_word(c, 1, "箸", "はし")
    _seed_ja_word(c, 2, "橋", "はし")
    _seed_ja_word(c, 3, "端", "はし")
    _seed_ja_word(c, 4, "雨", "あめ")
    _seed_ja_word(c, 5, "飴", "あめ")
    _seed_ja_word(c, 6, "寿司", "すし")
    _seed_kana_only(c, 7, "ああいう")
    # a non-Japanese lexeme with a coincidental kana-looking reading must NEVER get an accent
    c.execute("INSERT INTO lexeme(id,variety,headword,reading) VALUES (8,'zh','橋','qiao2')")
    c.execute("INSERT INTO lexeme_reading(lexeme_id,kind,value) VALUES (8,'kana','はし')")
    c.commit()
    yield c
    c.close()


def _accent(conn, kana, lexeme_id):
    row = conn.execute(
        "SELECT accent FROM lexeme_reading WHERE lexeme_id=? AND kind='kana' AND value=?",
        (lexeme_id, kana),
    ).fetchone()
    return row[0] if row else None


# --- parse() unit tests ---

# 1. POS tags are stripped, numeric downstep list preserved in order.
def test_parse_strips_pos_tags():
    by_pair, _ = accents.parse("二つ\tふたつ\t(副)0,(名)3")
    assert by_pair[("二つ", "ふたつ")] == "0,3"


# 2. kana-only rows (empty kana field) land in the kana map under the first field.
def test_parse_kana_only_map():
    _, by_kana = accents.parse("ああいう\t\t0")
    assert by_kana["ああいう"] == "0"


# --- minimal-pair correctness on the built fixture DB ---

# 3. 箸/はし → atamadaka (downstep 1).
def test_hashi_chopsticks_atamadaka(conn):
    accents.ingest(conn)
    assert _accent(conn, "はし", 1) == "1"


# 4. 橋/はし → odaka (downstep 2) and 端/はし → heiban (0): the kanji disambiguates the homograph.
def test_hashi_bridge_odaka_and_edge_heiban(conn):
    accents.ingest(conn)
    assert _accent(conn, "はし", 2) == "2"
    assert _accent(conn, "はし", 3) == "0"


# 5. 雨/あめ → atamadaka (1), 飴/あめ → heiban (0).
def test_ame_rain_vs_candy(conn):
    accents.ingest(conn)
    assert _accent(conn, "あめ", 4) == "1"
    assert _accent(conn, "あめ", 5) == "0"


# 6. multi-accent word keeps the full comma list.
def test_multi_accent_kept(conn):
    accents.ingest(conn)
    assert _accent(conn, "すし", 6) == "2,1"


# 7. kana-only word matches via the kana fallback.
def test_kana_only_word_matches(conn):
    accents.ingest(conn)
    assert _accent(conn, "ああいう", 7) == "0"


# 8. a non-Japanese (zh) reading is never touched, even with a coincidental kana value.
def test_non_japanese_untouched(conn):
    accents.ingest(conn)
    assert _accent(conn, "はし", 8) is None


# 9. idempotency: running twice yields identical accent state (and the same matched count).
def test_idempotent(conn):
    accents.ingest(conn)
    snapshot = conn.execute(
        "SELECT lexeme_id, value, accent FROM lexeme_reading WHERE kind='kana' ORDER BY lexeme_id"
    ).fetchall()
    n1 = conn.execute("SELECT count(*) FROM lexeme_reading WHERE kind='kana' AND accent IS NOT NULL").fetchone()[0]
    accents.ingest(conn)
    snapshot2 = conn.execute(
        "SELECT lexeme_id, value, accent FROM lexeme_reading WHERE kind='kana' ORDER BY lexeme_id"
    ).fetchall()
    n2 = conn.execute("SELECT count(*) FROM lexeme_reading WHERE kind='kana' AND accent IS NOT NULL").fetchone()[0]
    assert snapshot == snapshot2
    assert n1 == n2 == 7


# 10. the ALTER-in path: ingest works even if the column doesn't pre-exist (live-DB scenario).
def test_adds_column_if_missing(tmp_path, monkeypatch):
    import sqlite3
    # build a DB then drop the accent column by rebuilding lexeme_reading without it
    c = create_db(tmp_path / "noacc.sqlite")
    src = tmp_path / "accents.txt"
    src.write_text("箸\tはし\t1", encoding="utf-8")
    monkeypatch.setattr(accents, "SRC", src)
    c.execute("DROP TABLE lexeme_reading")
    c.execute(
        "CREATE TABLE lexeme_reading (lexeme_id INTEGER NOT NULL, kind TEXT NOT NULL, "
        "value TEXT NOT NULL, PRIMARY KEY (lexeme_id, kind, value)) WITHOUT ROWID"
    )
    _seed_ja_word(c, 1, "箸", "はし")
    c.commit()
    assert "accent" not in {r[1] for r in c.execute("PRAGMA table_info(lexeme_reading)")}
    accents.ingest(c)
    assert "accent" in {r[1] for r in c.execute("PRAGMA table_info(lexeme_reading)")}
    assert _accent(c, "はし", 1) == "1"
    c.close()
