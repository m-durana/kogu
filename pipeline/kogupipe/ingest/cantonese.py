"""Phase 3.3 - Cantonese layer (DESIGN.md §2.2).

Two CC-Canto files:
  * cccedict-canto-readings - jyutping for CC-CEDICT entries → attach jyutping to the shared zh
    lexemes (so Cantonese pronunciation shows on standard written vocabulary).
  * cccanto - a Cantonese dictionary incl. colloquial words and 粵字 (係 唔 嘅 喺 咗 冇 嘢 …) →
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

    retag(conn)


# --- Cantonese 粵字 retag --------------------------------------------------------------------------
# CC-CEDICT absorbed many Cantonese-only characters as zh entries with a *nominal* Mandarin reading
# (冇→mǎo, 喺→xí), so they wrongly surface as 中. But CC-CEDICT tags them in the gloss: "(Cantonese)"
# (often "Mandarin equivalent: …"). We relabel fully-Cantonese entries to 粵 with jyutping, and split
# genuinely-mixed homographs (係 = literary Mandarin "to relate" + Cantonese hai6 "to be") into a zh
# lexeme (Mandarin senses) + a yue lexeme (Cantonese senses). Re-runnable / idempotent.
RETAG_VERSION = "1"
# bland-gloss stragglers that are really Cantonese-only (gloss doesn't say "Cantonese")
ALLOWLIST_CANTO = {"嚟"}


def _is_canto_sense(g: str) -> bool:
    # markers that assert "this sense IS a Cantonese word": the literal "(Cantonese)", a "Cantonese
    # particle", or a "Mandarin equivalent: …" note. Deliberately NOT a bare "cantonese" substring,
    # which also matches etymology/context notes ("loanword via Cantonese", "in Cantonese cooking",
    # "overseas Cantonese communities") on words that are genuinely Mandarin (基, 夏娃, 曲奇, 點心, 唐人).
    s = (g or "").lower()
    return "(cantonese)" in s or "cantonese particle" in s or "mandarin equivalent" in s


def _char_jyut(conn, ch: str) -> str | None:
    row = conn.execute(
        "SELECT value FROM char_reading WHERE cp=? AND kind='jyutping' LIMIT 1", (ord(ch),)
    ).fetchone()
    return row[0] if row else None


def _jyutping_for(conn, lid: int, headword: str) -> str | None:
    """Existing jyutping on the lexeme, else assemble per-character from char_reading."""
    row = conn.execute(
        "SELECT value FROM lexeme_reading WHERE lexeme_id=? AND kind='jyutping' LIMIT 1", (lid,)
    ).fetchone()
    if row:
        return row[0]
    parts = []
    for ch in headword:
        j = _char_jyut(conn, ch)
        if not j:
            return None
        parts.append(j)
    return " ".join(parts) if parts else None


def retag(conn) -> None:
    cur = conn.execute("SELECT value FROM build_meta WHERE key='cantonese_retag'").fetchone()
    if cur and cur[0] == RETAG_VERSION:
        return

    existing_yue = {h for (h,) in conn.execute("SELECT DISTINCT headword FROM lexeme WHERE variety='yue'")}
    zh: dict[int, dict] = {}
    for lid, head in conn.execute("SELECT id, headword FROM lexeme WHERE variety='zh'"):
        zh[lid] = {"head": head, "senses": []}
    for sid, lid, gloss in conn.execute(
        "SELECT s.id, s.lexeme_id, s.gloss_en FROM sense s JOIN lexeme l ON l.id=s.lexeme_id WHERE l.variety='zh'"
    ):
        if lid in zh:
            zh[lid]["senses"].append((sid, gloss))

    next_lex = conn.execute("SELECT COALESCE(MAX(id),0) FROM lexeme").fetchone()[0]
    next_sf = conn.execute("SELECT COALESCE(MAX(id),0) FROM surface_form").fetchone()[0]
    relabeled = split = 0

    def _set_jyut(lid, jyut):
        if jyut:
            conn.execute("INSERT OR IGNORE INTO lexeme_reading(lexeme_id,kind,value) VALUES (?,?,?)", (lid, "jyutping", jyut))
            conn.execute("INSERT OR IGNORE INTO lexeme_reading(lexeme_id,kind,value) VALUES (?,?,?)", (lid, "jyutping_plain", _jyut_plain(jyut)))

    for lid, info in zh.items():
        head, senses = info["head"], info["senses"]
        if not senses:
            continue
        canto_sids = [sid for sid, g in senses if _is_canto_sense(g)]
        forced = head in ALLOWLIST_CANTO
        if not canto_sids and not forced:
            continue
        jyut = _jyutping_for(conn, lid, head)
        if len(canto_sids) == len(senses) or forced:
            # fully Cantonese → relabel in place to yue, swap the nominal Mandarin reading for jyutping
            conn.execute("UPDATE lexeme SET variety='yue', reading=? WHERE id=?", (jyut, lid))
            conn.execute(
                "DELETE FROM lexeme_reading WHERE lexeme_id=? AND kind IN ('pinyin','pinyin_num','pinyin_plain')", (lid,)
            )
            _set_jyut(lid, jyut)
            relabeled += 1
        else:
            # genuinely mixed → move the Cantonese sense(s) to a new yue lexeme, keep zh for Mandarin
            if head in existing_yue:
                continue
            next_lex += 1
            yid = next_lex
            conn.execute(
                "INSERT INTO lexeme(id,variety,headword,reading,freq,freq_source) VALUES (?,?,?,?,?,?)",
                (yid, "yue", head, jyut, None, None),
            )
            for form, script, region, isp in conn.execute(
                "SELECT form,script,region,is_primary FROM surface_form WHERE lexeme_id=?", (lid,)
            ).fetchall():
                next_sf += 1
                conn.execute(
                    "INSERT INTO surface_form(id,lexeme_id,form,script,region,is_primary) VALUES (?,?,?,?,?,?)",
                    (next_sf, yid, form, script, region, isp),
                )
            for sid in canto_sids:
                conn.execute("UPDATE sense SET lexeme_id=? WHERE id=?", (yid, sid))
            _set_jyut(yid, jyut)
            existing_yue.add(head)
            split += 1

    conn.execute("INSERT OR REPLACE INTO build_meta(key,value) VALUES ('cantonese_retag',?)", (RETAG_VERSION,))
    print(f"      cantonese retag: relabeled={relabeled}, split-mixed={split}")
