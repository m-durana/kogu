"""Phase 3.3 — Cantonese layer (DESIGN.md §2.2).

Two CC-Canto files:
  * cccedict-canto-readings — jyutping for CC-CEDICT entries → attach jyutping to the shared zh
    lexemes (so Cantonese pronunciation shows on standard written vocabulary).
  * cccanto — a Cantonese dictionary incl. colloquial words and 粵字 (係 唔 嘅 喺 咗 冇 嘢 …) →
    create first-class `yue` lexemes for the entries not already standard Mandarin words.

Runs after lexemes, before concepts (so yue lexemes also join the gloss-pivot concept layer).
"""
from __future__ import annotations

import re
import zipfile

from ..db import SOURCES_DIR

CCCANTO = SOURCES_DIR / "cccanto.zip"
READINGS = SOURCES_DIR / "cccanto-readings.zip"

# trad simp [pinyin] {jyutping} [/gloss/gloss/...]   (gloss optional; optional trailing " # comment")
_LINE = re.compile(r"^(\S+)\s+(\S+)\s+\[([^\]]*)\]\s+\{([^}]*)\}(?:\s+/(.*?)/)?\s*(?:#.*)?$")
_DIGITS = re.compile(r"\d")


def _jyut_plain(j: str) -> str:
    return _DIGITS.sub("", j.lower().replace(" ", ""))


def _lines(zip_path):
    with zipfile.ZipFile(zip_path) as z:
        name = next(n for n in z.namelist() if n.endswith(".txt"))
        for raw in z.read(name).decode("utf-8").splitlines():
            if not raw or raw.startswith("#"):
                continue
            m = _LINE.match(raw)
            if m:
                trad, simp, pinyin, jyut, gloss = m.groups()
                glosses = [g for g in (gloss or "").split("/") if g]
                yield trad, simp, pinyin, jyut.strip(), glosses


def ingest(conn) -> None:
    # --- attach jyutping to existing zh lexemes (shared vocabulary) ---
    zh_by_head: dict[str, list[int]] = {}
    for lid, head in conn.execute("SELECT id, headword FROM lexeme WHERE variety='zh'"):
        zh_by_head.setdefault(head, []).append(lid)

    reading_rows = []
    attached = 0
    for trad, _simp, _pinyin, jyut, _gl in _lines(READINGS):
        if not jyut:
            continue
        for lid in zh_by_head.get(trad, []):
            reading_rows.append((lid, "jyutping", jyut))
            reading_rows.append((lid, "jyutping_plain", _jyut_plain(jyut)))
            attached += 1
    conn.executemany(
        "INSERT OR IGNORE INTO lexeme_reading(lexeme_id,kind,value) VALUES (?,?,?)", reading_rows)

    # --- create yue lexemes for Cantonese-specific entries (not standard Mandarin words) ---
    existing_zh = set(zh_by_head)
    next_lex = conn.execute("SELECT COALESCE(MAX(id),0) FROM lexeme").fetchone()[0]
    next_sf = conn.execute("SELECT COALESCE(MAX(id),0) FROM surface_form").fetchone()[0]
    next_sense = conn.execute("SELECT COALESCE(MAX(id),0) FROM sense").fetchone()[0]

    lex, forms, readings, senses = [], [], [], []
    created = 0
    for trad, simp, _pinyin, jyut, glosses in _lines(CCCANTO):
        if not glosses or trad in existing_zh:
            continue  # shared vocab already covered (+ got jyutping above)
        next_lex += 1
        lid = next_lex
        lex.append((lid, "yue", trad, jyut or None, None, None))
        next_sf += 1
        forms.append((next_sf, lid, trad, "trad", "HK", 1))
        if simp != trad:
            next_sf += 1
            forms.append((next_sf, lid, simp, "simp", "CN", 0))
        if jyut:
            readings.append((lid, "jyutping", jyut))
            readings.append((lid, "jyutping_plain", _jyut_plain(jyut)))
        next_sense += 1
        senses.append((next_sense, lid, None, "; ".join(glosses), 0))
        created += 1

    conn.executemany("INSERT INTO lexeme(id,variety,headword,reading,freq,freq_source) VALUES (?,?,?,?,?,?)", lex)
    conn.executemany("INSERT INTO surface_form(id,lexeme_id,form,script,region,is_primary) VALUES (?,?,?,?,?,?)", forms)
    conn.executemany("INSERT OR IGNORE INTO lexeme_reading(lexeme_id,kind,value) VALUES (?,?,?)", readings)
    conn.executemany("INSERT INTO sense(id,lexeme_id,pos,gloss_en,sense_order) VALUES (?,?,?,?,?)", senses)
    print(f"      jyutping attached to zh={attached}, yue lexemes created={created}")
