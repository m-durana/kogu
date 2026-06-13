"""Phase 1.2 — lexeme ingestion.

Chinese (CC-CEDICT): one lexeme with two skins (trad + simp surface forms, one Mandarin reading).
Japanese (JMdict-simplified): separate lexemes — never merged into Chinese (that merge is what
produced the 会社 false friend). Region tags use the core-four launch set; fine-grained regional
vocabulary splits are Phase 3 (the columns exist now).

Pinyin is stored raw plus normalised forms (numbered, toneless) so the query layer can match
tone-marks / tone-numbers / toneless input without per-query work.
"""
from __future__ import annotations

import gzip
import json
import re
import zipfile

from ..db import SOURCES_DIR

CEDICT = SOURCES_DIR / "cedict.txt.gz"
JMDICT_ZIP = SOURCES_DIR / "jmdict-eng-common.json.zip"

_CEDICT_RE = re.compile(r"^(\S+)\s+(\S+)\s+\[([^\]]*)\]\s+/(.*)/\s*$")
_TONE = re.compile(r"[1-5]")
# CC-CEDICT uses 'xx5' as a placeholder for an unknown reading. The original CJKV Dict leaked
# this to the UI — we suppress it (keep the word + glosses, drop the bogus reading).
_PLACEHOLDER = re.compile(r"xx[0-9]", re.IGNORECASE)


def _pinyin_forms(syllables: str) -> tuple[str, str, str]:
    """('ji1 chang3') -> (raw, numbered 'ji1chang3', toneless 'jichang')."""
    raw = syllables.strip()
    low = raw.lower().replace("u:", "v")
    numbered = low.replace(" ", "")
    toneless = _TONE.sub("", numbered)
    return raw, numbered, toneless


class _Ids:
    def __init__(self):
        self.lex = self.sf = self.sense = 0

    def next_lex(self):
        self.lex += 1
        return self.lex

    def next_sf(self):
        self.sf += 1
        return self.sf

    def next_sense(self):
        self.sense += 1
        return self.sense


def _ingest_cedict(conn, ids: _Ids) -> int:
    lexemes, forms, readings, senses = [], [], [], []
    n = 0
    with gzip.open(CEDICT, "rt", encoding="utf-8") as f:
        for line in f:
            if line.startswith("#") or not line.strip():
                continue
            m = _CEDICT_RE.match(line)
            if not m:
                continue
            trad, simp, pinyin, gloss_blob = m.groups()
            glosses = [g for g in gloss_blob.split("/") if g]
            if not glosses:
                continue
            placeholder = bool(_PLACEHOLDER.search(pinyin))
            raw, numbered, toneless = _pinyin_forms(pinyin)
            lid = ids.next_lex()
            lexemes.append((lid, "zh", trad, None if placeholder else raw, None, None))
            # surface forms: traditional (general) + simplified (mainland)
            forms.append((ids.next_sf(), lid, trad, "trad", None, 1))
            if simp != trad:
                forms.append((ids.next_sf(), lid, simp, "simp", "CN", 0))
            if not placeholder:  # never leak the xx5 placeholder reading
                readings.append((lid, "pinyin", raw))
                readings.append((lid, "pinyin_num", numbered))
                readings.append((lid, "pinyin_plain", toneless))
            senses.append((ids.next_sense(), lid, None, "; ".join(glosses), 0))
            n += 1
    conn.executemany("INSERT INTO lexeme(id,variety,headword,reading,freq,freq_source) VALUES (?,?,?,?,?,?)", lexemes)
    conn.executemany("INSERT INTO surface_form(id,lexeme_id,form,script,region,is_primary) VALUES (?,?,?,?,?,?)", forms)
    conn.executemany("INSERT OR IGNORE INTO lexeme_reading(lexeme_id,kind,value) VALUES (?,?,?)", readings)
    conn.executemany("INSERT INTO sense(id,lexeme_id,pos,gloss_en,sense_order) VALUES (?,?,?,?,?)", senses)
    return n


def _ingest_jmdict(conn, ids: _Ids) -> int:
    z = zipfile.ZipFile(JMDICT_ZIP)
    name = next(n for n in z.namelist() if n.endswith(".json"))
    data = json.loads(z.read(name))
    lexemes, forms, readings, senses = [], [], [], []
    n = 0
    for w in data["words"]:
        kanji = w.get("kanji", [])
        kana = w.get("kana", [])
        if not kana:
            continue
        headword = kanji[0]["text"] if kanji else kana[0]["text"]
        primary_reading = kana[0]["text"]
        common = any(k.get("common") for k in kanji) or any(k.get("common") for k in kana)
        lid = ids.next_lex()
        lexemes.append((lid, "ja", headword, primary_reading, 1.0 if common else 0.3, "jmdict-common"))
        for i, k in enumerate(kanji):
            forms.append((ids.next_sf(), lid, k["text"], "shinjitai", "JP", 1 if i == 0 else 0))
        for i, kn in enumerate(kana):
            # kana form is primary only for kana-only words (no kanji), first kana
            is_primary = 1 if (not kanji and i == 0) else 0
            forms.append((ids.next_sf(), lid, kn["text"], "kana", "JP", is_primary))
            readings.append((lid, "kana", kn["text"]))
        order = 0
        for s in w.get("sense", []):
            gl = [g["text"] for g in s.get("gloss", []) if g.get("lang") == "eng"]
            if not gl:
                continue
            pos = ",".join(s.get("partOfSpeech", []))
            senses.append((ids.next_sense(), lid, pos or None, "; ".join(gl), order))
            order += 1
        n += 1
    conn.executemany("INSERT INTO lexeme(id,variety,headword,reading,freq,freq_source) VALUES (?,?,?,?,?,?)", lexemes)
    conn.executemany("INSERT INTO surface_form(id,lexeme_id,form,script,region,is_primary) VALUES (?,?,?,?,?,?)", forms)
    conn.executemany("INSERT OR IGNORE INTO lexeme_reading(lexeme_id,kind,value) VALUES (?,?,?)", readings)
    conn.executemany("INSERT INTO sense(id,lexeme_id,pos,gloss_en,sense_order) VALUES (?,?,?,?,?)", senses)
    return n


def ingest(conn) -> None:
    ids = _Ids()
    nz = _ingest_cedict(conn, ids)
    nj = _ingest_jmdict(conn, ids)
    print(f"      zh lexemes={nz} ja lexemes={nj}")
