"""Phase 2.2 - explicit cross-variety equivalence edges (lexeme_equivalent).

Three reliable sources, in increasing trust:
  1. CC-Canto inline "(Mandarin equivalent: 沒有|没有[...])" notes already in our Cantonese glosses -
     a precise lexicographer statement of the colloquial-Cantonese -> standard-Chinese bridge.
  2. curated/equivalents_yue_zh.tsv  - hand-verified 粵-colloquial -> 中-standard pairs.
  3. curated/bridges_crosslang.tsv   - hand-verified zh / yue / ja "same meaning, different word"
     triples (機場 / - / 空港), which the fuzzy English-gloss-pivot concept layer misses.

Every form is resolved to an existing lexeme of the right variety; unresolved rows are skipped (and
counted) so a typo in the curated data can never invent a link. Idempotent: clears + rebuilds the
table, so it is safe to re-run against the live DB.
"""
from __future__ import annotations

import csv
import re
from pathlib import Path

_CURATED = Path(__file__).resolve().parents[2] / "curated"

# "Mandarin equivalent: 沒有|没有[mei2 you3]"  /  "(Mandarin equivalent: 的)"
_EQUIV_RE = re.compile(r"Mandarin equivalent:\s*([^\[\]()（）;；]+)")


def _ensure_table(conn) -> None:
    conn.execute(
        "CREATE TABLE IF NOT EXISTS lexeme_equivalent ("
        " src_lexeme_id INTEGER NOT NULL,"
        " dst_lexeme_id INTEGER NOT NULL,"
        " relation TEXT NOT NULL,"
        " source TEXT NOT NULL,"
        " PRIMARY KEY (src_lexeme_id, dst_lexeme_id, relation)) WITHOUT ROWID"
    )
    conn.execute("CREATE INDEX IF NOT EXISTS idx_lex_equiv_src ON lexeme_equivalent(src_lexeme_id)")
    conn.execute("CREATE INDEX IF NOT EXISTS idx_lex_equiv_dst ON lexeme_equivalent(dst_lexeme_id)")


def _resolver(conn):
    """Return resolve(form, variety) -> lexeme_id | None, picking the richest/commonest match."""
    cache: dict[tuple[str, str], int | None] = {}

    def resolve(form: str, variety: str) -> int | None:
        form = (form or "").strip()
        if not form:
            return None
        key = (form, variety)
        if key in cache:
            return cache[key]
        # prefer the lexeme this form is the HEADWORD of, then one where it's a primary form - so a
        # form that is merely the simplified alias of another lexeme (家 is the simp of 傢) never wins
        # over the lexeme actually headed by that form.
        row = conn.execute(
            "SELECT l.id FROM lexeme l JOIN surface_form sf ON sf.lexeme_id = l.id "
            "WHERE l.variety = ?2 AND sf.form = ?1 "
            # a kana surface form is a READING alias of a kanji word (蛇 carries form じゃ) unless it IS
            # the headword (a genuine kana word). Resolving foreign targets to a reading produced false
            # bridges (bye → じゃ → 蛇 'snake'), so only accept a kana form when it is the headword.
            "  AND (sf.script <> 'kana' OR l.headword = ?1) "
            "GROUP BY l.id "
            "ORDER BY (l.headword = ?1) DESC, MAX(sf.is_primary) DESC, "
            "         (SELECT COUNT(*) FROM sense s WHERE s.lexeme_id = l.id) DESC, "
            "         l.freq IS NULL, l.freq DESC, l.id ASC LIMIT 1",
            (form, variety),
        ).fetchone()
        cache[key] = row[0] if row else None
        return cache[key]

    return resolve


def _first_form(field: str) -> str:
    """A curated cell like '沒有|没有' or '沒有 / 没有' -> the first (traditional) form."""
    return re.split(r"[|/／]", field.strip())[0].strip()


def ingest(conn) -> None:
    _ensure_table(conn)
    conn.execute("DELETE FROM lexeme_equivalent")
    resolve = _resolver(conn)
    edges: set[tuple[int, int, str, str]] = set()

    # 1. CC-Canto inline "Mandarin equivalent:" notes on yue senses.
    inline_hit = inline_miss = 0
    for lex_id, gloss in conn.execute(
        "SELECT s.lexeme_id, s.gloss_en FROM sense s JOIN lexeme l ON l.id = s.lexeme_id "
        "WHERE l.variety = 'yue' AND s.gloss_en LIKE '%Mandarin equivalent%'"
    ):
        m = _EQUIV_RE.search(gloss or "")
        if not m:
            continue
        zh_id = resolve(_first_form(m.group(1)), "zh")
        if zh_id and zh_id != lex_id:
            edges.add((lex_id, zh_id, "colloquial-standard", "cc-canto-inline"))
            inline_hit += 1
        else:
            inline_miss += 1

    # 2. curated 粵 colloquial -> 中 standard.
    cur_hit = cur_miss = 0
    yz = _CURATED / "equivalents_yue_zh.tsv"
    if yz.exists():
        for r in _rows(yz):
            yue_id = resolve(_first_form(r.get("yue_form", "")), "yue")
            zh_id = resolve(_first_form(r.get("zh_equiv_trad", "")), "zh")
            if yue_id and zh_id and yue_id != zh_id:
                edges.add((yue_id, zh_id, "colloquial-standard", "curated"))
                cur_hit += 1
            else:
                cur_miss += 1

    # 3. curated cross-language triples -> pairwise edges among the present varieties.
    xl_hit = xl_miss = 0
    xl = _CURATED / "bridges_crosslang.tsv"
    if xl.exists():
        for r in _rows(xl):
            members: list[tuple[int, str]] = []  # (lexeme_id, form)
            seen_ids: set[int] = set()
            for col, var in (("zh_trad", "zh"), ("yue_trad", "yue"), ("ja", "ja")):
                form = _first_form(r.get(col, ""))
                lid = resolve(form, var)
                if lid and lid not in seen_ids:
                    seen_ids.add(lid)
                    members.append((lid, form))
            made = False
            for a_id, a_form in members:
                for b_id, b_form in members:
                    # only a real bridge if the two varieties write it DIFFERENTLY - skip shared
                    # glyphs (zh 帽子 == ja 帽子), which aren't "written differently".
                    if a_id != b_id and a_form != b_form:
                        edges.add((a_id, b_id, "cross-lang", "curated"))
                        made = True
            xl_hit += 1 if made else 0
            xl_miss += 0 if made else 1

    conn.executemany(
        "INSERT OR IGNORE INTO lexeme_equivalent(src_lexeme_id,dst_lexeme_id,relation,source) "
        "VALUES (?,?,?,?)",
        list(edges),
    )
    print(
        f"      equivalents: inline={inline_hit}(miss {inline_miss}) "
        f"curated-yz={cur_hit}(miss {cur_miss}) cross-lang={xl_hit}(miss {xl_miss}) "
        f"edges={len(edges)}"
    )


def _rows(path: Path):
    """Yield dict rows from a TSV with a header, skipping blank/comment lines."""
    with path.open(encoding="utf-8") as f:
        lines = [ln for ln in f if ln.strip() and not ln.lstrip().startswith("#")]
    yield from csv.DictReader(lines, delimiter="\t")
