"""Phase 1.1 - character backbone ingestion (DESIGN.md §2.1).

Sources: Unihan (readings, variants, strokes, radical, gloss), cjkvi-ids (IDS decomposition),
OpenCC (STCharacters simp→trad, JPShinjitaiCharacters shinjitai→kyūjitai, TW/HK variants).

Produces the orthodox graph: every living glyph carries a reform-tagged edge to its orthodox
parent. identity-class edges = simplification / shinjitai / z-variant / region-standard;
semantic-variant edges are suggestion-only (never used for orthodox resolution).
"""
from __future__ import annotations

import json
import zipfile
from collections import defaultdict

from ..db import SOURCES_DIR

UNIHAN_ZIP = SOURCES_DIR / "Unihan.zip"
KANJIDIC_ZIP = SOURCES_DIR / "kanjidic2-en.json.zip"


def _kanjidic_kana():
    """codepoint -> (onyomi[katakana], kunyomi[hiragana]) from Kanjidic - proper kana, not romaji."""
    on: dict[int, list[str]] = {}
    kun: dict[int, list[str]] = {}
    if not KANJIDIC_ZIP.exists():
        return on, kun
    with zipfile.ZipFile(KANJIDIC_ZIP) as z:
        name = next(n for n in z.namelist() if n.endswith(".json"))
        data = json.loads(z.read(name))
    for c in data.get("characters", []):
        lit = c.get("literal")
        if not lit or len(lit) != 1:
            continue
        cp = ord(lit)
        rm = c.get("readingMeaning") or {}
        for g in rm.get("groups", []):
            for r in g.get("readings", []):
                if r.get("type") == "ja_on":
                    on.setdefault(cp, []).append(r["value"])
                elif r.get("type") == "ja_kun":
                    kun.setdefault(cp, []).append(r["value"])
    return on, kun


def _unihan(file: str):
    """Yield (codepoint:int, field:str, value:str) from a Unihan_*.txt inside the zip."""
    with zipfile.ZipFile(UNIHAN_ZIP) as z:
        for raw in z.read(file).decode("utf-8").splitlines():
            if not raw or raw.startswith("#"):
                continue
            cp_s, field, value = raw.split("\t", 2)
            yield int(cp_s[2:], 16), field, value


def _opencc(name: str):
    """Yield (key:str, [values]) from an OpenCC dictionary file (skips comments)."""
    path = SOURCES_DIR / name
    for raw in path.read_text(encoding="utf-8").splitlines():
        if not raw or raw.startswith("#"):
            continue
        parts = raw.split("\t")
        if len(parts) < 2:
            continue
        yield parts[0], parts[1].split(" ")


def _cp(ch: str) -> int | None:
    return ord(ch) if len(ch) == 1 else None


def ingest(conn) -> None:
    # ---- collect per-character data ----
    readings: dict[int, dict[str, list[str]]] = defaultdict(lambda: defaultdict(list))
    gloss: dict[int, str] = {}
    strokes: dict[int, int] = {}
    radical: dict[int, int] = {}
    ids: dict[int, str] = {}
    chars: set[int] = set()  # every codepoint we will materialise as a character row

    # NB: Unihan kJapaneseOn/kJapaneseKun are ROMAJI (e.g. "ABURA", "KO") - unusable for kana
    # display, so we do NOT ingest them. Japanese on/kun come solely from Kanjidic (proper kana,
    # applied below); characters Kanjidic lacks simply get no JP reading rather than romaji junk.
    READING_FIELDS = {
        "kMandarin": "pinyin", "kCantonese": "jyutping",
    }
    for cp, field, value in _unihan("Unihan_Readings.txt"):
        if field in READING_FIELDS:
            kind = READING_FIELDS[field]
            for v in value.split(" "):
                if v:
                    readings[cp][kind].append(v)
            chars.add(cp)
        elif field == "kDefinition":
            gloss[cp] = value
            chars.add(cp)

    for cp, field, value in _unihan("Unihan_IRGSources.txt"):
        if field == "kTotalStrokes":
            try:
                strokes[cp] = int(value.split(" ")[0])
                chars.add(cp)
            except ValueError:
                pass

    for cp, field, value in _unihan("Unihan_RadicalStrokeCounts.txt"):
        if field == "kRSUnicode":
            head = value.split(" ")[0].split(".")[0].rstrip("'")
            try:
                radical[cp] = int(head)
                chars.add(cp)
            except ValueError:
                pass

    # cjkvi-ids: "U+5B66\t学\t⿳𭕄冖子 ..."
    for raw in (SOURCES_DIR / "ids.txt").read_text(encoding="utf-8").splitlines():
        if not raw or raw.startswith("#") or raw.startswith(";;"):
            continue
        cols = raw.split("\t")
        if len(cols) >= 3 and cols[0].startswith("U+"):
            try:
                cp = int(cols[0][2:], 16)
            except ValueError:
                continue
            ids[cp] = cols[2].strip()
            chars.add(cp)

    # ---- override Japanese on/kun with proper kana from Kanjidic (Unihan only has romaji) ----
    kd_on, kd_kun = _kanjidic_kana()
    for cp, vals in kd_on.items():
        chars.add(cp)
        readings[cp]["onyomi"] = list(dict.fromkeys(vals))
    for cp, vals in kd_kun.items():
        chars.add(cp)
        readings[cp]["kunyomi"] = list(dict.fromkeys(vals))

    # ---- collect edges: (child_cp, parent_cp, type, reform_id) ----
    edges: list[tuple[int, int, str, str]] = []

    def add_edge(child: str, parent: str, etype: str, reform: str):
        ccp, pcp = _cp(child), _cp(parent)
        if ccp is None or pcp is None or ccp == pcp:
            return
        chars.add(ccp)
        chars.add(pcp)
        edges.append((ccp, pcp, etype, reform))

    # OpenCC simp → trad (authoritative; carries the merges, e.g. 发 → 發 髮)
    for simp, trads in _opencc("STCharacters.txt"):
        for trad in trads:
            add_edge(simp, trad, "simplification", "opencc")
    # OpenCC shinjitai → kyūjitai (this is where 広 → 廣 lives - the Unihan gap)
    for shin, kyus in _opencc("JPShinjitaiCharacters.txt"):
        for kyu in kyus:
            add_edge(shin, kyu, "shinjitai", "jp-toyo")
    # OpenCC regional standards (TW/HK): key is the common form, value the regional preferred
    for std, regs in _opencc("TWVariants.txt"):
        for r in regs:
            add_edge(r, std, "region-standard", "tw-std")
    for std, regs in _opencc("HKVariants.txt"):
        for r in regs:
            add_edge(r, std, "region-standard", "hk-std")

    # Unihan variants (complement OpenCC; semantic = suggestion only)
    UNIHAN_EDGE = {
        "kSimplifiedVariant": ("simplification", "unihan-variant", "child"),   # trad -> simp
        "kTraditionalVariant": ("simplification", "unihan-variant", "parent"),  # simp -> trad
        "kZVariant": ("z-variant", "unihan-variant", "parent"),
        "kSemanticVariant": ("semantic-variant", "unihan-variant", "parent"),
        "kSpecializedSemanticVariant": ("semantic-variant", "unihan-variant", "parent"),
    }
    for cp, field, value in _unihan("Unihan_Variants.txt"):
        spec = UNIHAN_EDGE.get(field)
        if not spec:
            continue
        etype, reform, role = spec
        this = chr(cp)
        for tok in value.split(" "):
            # token like "U+5B66" or "U+5B66<kMatthews"
            code = tok.split("<")[0]
            if not code.startswith("U+"):
                continue
            other = chr(int(code[2:], 16))
            if role == "child":          # this(trad) -> other(simp): other is child
                add_edge(other, this, etype, reform)
            else:                         # this(simp/variant) -> other(orthodox)
                add_edge(this, other, etype, reform)

    # ---- break identity-edge cycles by source precedence ----
    # A handful of near-identical glyph pairs (凜/凛, 稜/棱) get contradictory directions from
    # different sources: OpenCC's simplification says one is orthodox, shinjitai says the other.
    # Resolve deterministically: lower rank = more authoritative for orthodox resolution; when
    # both directions of a pair exist, keep the winning direction and drop the loser. This makes
    # the bounded transitive closure terminate (the build invariant).
    RANK = {("simplification", "opencc"): 0, ("shinjitai", "jp-toyo"): 1,
            ("simplification", "unihan-variant"): 2, ("shinjitai", "unihan-variant"): 3}

    identity = [e for e in edges if e[2] in ("simplification", "shinjitai")]
    other = [e for e in edges if e[2] not in ("simplification", "shinjitai")]
    by_pair: dict[frozenset, list] = defaultdict(list)
    for e in identity:
        by_pair[frozenset((e[0], e[1]))].append(e)
    kept_identity = []
    for pair, es in by_pair.items():
        dirs = {(e[0], e[1]) for e in es}
        if len(dirs) > 1:  # both directions present -> keep only the winning direction
            best = min(es, key=lambda e: RANK.get((e[2], e[3]), 9))
            win = (best[0], best[1])
            es = [e for e in es if (e[0], e[1]) == win]
        kept_identity.extend(es)
    edges = other + kept_identity

    # ---- determine orthodox: a glyph is derived if it is the child of a simp/shinjitai edge ----
    derived = {c for (c, _p, t, _r) in edges if t in ("simplification", "shinjitai")}

    # ---- write characters ----
    char_rows = [
        (cp, chr(cp), 0 if cp in derived else 1,
         strokes.get(cp), radical.get(cp), ids.get(cp), gloss.get(cp))
        for cp in sorted(chars)
        if 0x3000 <= cp <= 0x3FFFF  # CJK ranges incl. ext; skip stray codepoints
    ]
    valid = {r[0] for r in char_rows}
    conn.executemany(
        "INSERT OR IGNORE INTO character(cp,char,is_orthodox,strokes,radical,ids,gloss_en) "
        "VALUES (?,?,?,?,?,?,?)", char_rows)

    # ---- write readings ----
    reading_rows = []
    for cp, kinds in readings.items():
        if cp not in valid:
            continue
        for kind, vals in kinds.items():
            for v in dict.fromkeys(vals):  # dedupe, keep order
                reading_rows.append((cp, kind, v))
    conn.executemany(
        "INSERT OR IGNORE INTO char_reading(cp,kind,value) VALUES (?,?,?)", reading_rows)

    # ---- write edges (skip any endpoint we didn't materialise) ----
    edge_rows = [(c, p, t, r) for (c, p, t, r) in edges if c in valid and p in valid]
    conn.executemany(
        "INSERT OR IGNORE INTO glyph_edge(child_cp,parent_cp,type,reform_id) VALUES (?,?,?,?)",
        edge_rows)

    print(f"      characters={len(char_rows)} readings={len(reading_rows)} edges(raw)={len(edge_rows)}")
