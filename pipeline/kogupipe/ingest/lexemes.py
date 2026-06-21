"""Phase 1.2 - lexeme ingestion.

Chinese (CC-CEDICT): one lexeme with two skins (trad + simp surface forms, one Mandarin reading).
Japanese (JMdict-simplified): separate lexemes - never merged into Chinese (that merge is what
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
JMDICT_ZIP = SOURCES_DIR / "jmdict-eng.json.zip"

# JMdict orthography tags we must not surface as a normal headword/variant: sK/sk = search-only
# (must not display), rK = rarely-used, iK = irregular, oK = outdated kanji.
_JM_SEARCH_ONLY = {"sK", "sk"}
_JM_NONSTD = {"rK", "iK", "oK", "sK", "sk"}


def _jm_tags(x: dict) -> set[str]:
    return set(x.get("tags", []))


def _jm_headword(kanji: list, kana: list) -> str:
    """Pick a sensible displayed headword: the first standard kanji (not rare/irregular/search-only);
    else, for usually-kana words, the first displayable kana; else any available form."""
    for k in kanji:
        if not (_jm_tags(k) & _JM_NONSTD):
            return k["text"]
    if kana:
        return kana[0]["text"]
    return kanji[0]["text"] if kanji else ""

_CEDICT_RE = re.compile(r"^(\S+)\s+(\S+)\s+\[([^\]]*)\]\s+/(.*)/\s*$")
_TONE = re.compile(r"[1-5]")

# CC-CEDICT embeds cross-references as `trad|simp[pin1yin1]` (or `word[pinyin]`, `trad|simp`) inside
# glosses. Left raw they leak the simplified twin and a tone-numbered romanisation into the prose, and
# rare referents render as tofu (〡 → "...蘇州碼子|苏州码子[Su1 zhou1 ma3 zi5]"). We keep the (traditional)
# headword and drop the `|simp` twin and `[pinyin]`. A bracket NOT glued to a preceding word (a real
# pronunciation note like "Tai-lo pr. [khè-su]") is left untouched.
_REF_FULL = re.compile(r"([^\s|\[\]]+)\|[^\s\[\]]+(?:\[[^\]]*\])?")
_REF_PIN = re.compile(r"([^\s|\[\]]+)(\[[^\]]*\])")
_WS = re.compile(r"\s{2,}")


def clean_gloss(g: str) -> str:
    """Strip CC-CEDICT `trad|simp[pinyin]` cross-reference markup, keeping the traditional headword."""
    g = _REF_FULL.sub(r"\1", g)
    g = _REF_PIN.sub(r"\1", g)
    return _WS.sub(" ", g).strip()
# CC-CEDICT uses 'xx5' as a placeholder for an unknown reading. The original CJKV Dict leaked
# this to the UI - we suppress it (keep the word + glosses, drop the bogus reading).
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
            # one sense row per CC-CEDICT '/'-delimited sense (NOT joined into one), so Chinese
            # entries enumerate like JMdict's Japanese ones instead of collapsing to a single "1.".
            # The '/' is CC-CEDICT's real sense boundary; ';' inside a sense stays (it marks synonyms,
            # exactly as JMdict uses it), so we never over-split.
            for order, g in enumerate(glosses):
                senses.append((ids.next_sense(), lid, None, clean_gloss(g), order))
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
        # drop search-only (sK/sk) forms entirely: they are meant to be matched, never displayed as
        # a headword or orthographic variant. rK/iK/oK are kept (real, if rare) but never the headword.
        disp_kanji = [k for k in kanji if not (_jm_tags(k) & _JM_SEARCH_ONLY)]
        disp_kana = [kn for kn in kana if not (_jm_tags(kn) & _JM_SEARCH_ONLY)] or kana[:1]
        headword = _jm_headword(disp_kanji, disp_kana)
        primary_reading = disp_kana[0]["text"]
        has_kanji_head = any(k["text"] == headword for k in disp_kanji)
        common = any(k.get("common") for k in kanji) or any(k.get("common") for k in kana)
        lid = ids.next_lex()
        lexemes.append((lid, "ja", headword, primary_reading, 1.0 if common else 0.3, "jmdict"))
        for k in disp_kanji:
            forms.append((ids.next_sf(), lid, k["text"], "shinjitai", "JP", 1 if k["text"] == headword else 0))
        for kn in disp_kana:
            # kana form is primary only when the word's headword is itself kana (usually-kana words)
            is_primary = 1 if (not has_kanji_head and kn["text"] == headword) else 0
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
