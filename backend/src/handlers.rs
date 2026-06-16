//! Axum handlers for the read-only API.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::model::*;
use crate::search;
use crate::state::AppState;

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "kogu", "version": env!("CARGO_PKG_VERSION") }))
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub script: Option<String>,
    pub limit: Option<usize>,
}

pub async fn search_handler(
    State(st): State<AppState>,
    Query(p): Query<SearchParams>,
) -> Result<Json<SearchResponse>, (StatusCode, Json<Value>)> {
    let conn = st.pool.get().map_err(internal)?;
    let limit = p.limit.unwrap_or(50).clamp(1, 200);
    let resp = search::search(&st, &conn, &p.q, p.script.as_deref(), limit).map_err(internal)?;
    Ok(Json(resp))
}

pub async fn entry_handler(
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Entry>, (StatusCode, Json<Value>)> {
    let conn = st.pool.get().map_err(internal)?;
    match build_entry(&st, &conn, id).map_err(internal)? {
        Some(e) => Ok(Json(e)),
        None => Err((StatusCode::NOT_FOUND, Json(json!({ "error": "not_found" })))),
    }
}

fn build_entry(
    st: &AppState,
    conn: &rusqlite::Connection,
    id: i64,
) -> rusqlite::Result<Option<Entry>> {
    // negative id = a character-only entry (kokuji / char with no word-lexeme), keyed by codepoint
    if id < 0 {
        return build_char_entry(conn, (-id) as u32);
    }
    let row = conn.query_row(
        "SELECT variety, headword, reading, freq FROM lexeme WHERE id = ?1",
        [id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<f64>>(3)?,
            ))
        },
    );
    let (variety, headword, reading, freq) = match row {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e),
    };

    // forms
    let mut s = conn.prepare(
        "SELECT form, script, region, is_primary FROM surface_form WHERE lexeme_id=?1 ORDER BY is_primary DESC",
    )?;
    let forms: Vec<Form> = s
        .query_map([id], |r| {
            Ok(Form {
                form: r.get(0)?,
                script: r.get(1)?,
                region: r.get(2)?,
                is_primary: r.get::<_, i64>(3)? != 0,
            })
        })?
        .collect::<Result<_, _>>()?;

    // readings (hide internal normalisation forms)
    let mut s = conn.prepare(
        "SELECT kind, value FROM lexeme_reading WHERE lexeme_id=?1 \
         AND kind NOT IN ('pinyin_num','pinyin_plain','jyutping_plain')",
    )?;
    let readings: Vec<ReadingKV> = s
        .query_map([id], |r| Ok(ReadingKV { kind: r.get(0)?, value: r.get(1)? }))?
        .collect::<Result<_, _>>()?;

    // senses
    let mut s = conn
        .prepare("SELECT pos, gloss_en FROM sense WHERE lexeme_id=?1 ORDER BY sense_order")?;
    let senses: Vec<Sense> = s
        .query_map([id], |r| Ok(Sense { pos: r.get(0)?, gloss_en: r.get(1)? }))?
        .collect::<Result<_, _>>()?;

    // per-character backbone (use the primary form, else headword)
    let primary = forms.iter().find(|f| f.is_primary).map(|f| f.form.clone()).unwrap_or(headword.clone());
    let mut characters = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for ch in primary.chars() {
        if !seen.insert(ch) {
            continue;
        }
        if let Some(ci) = char_info(conn, ch)? {
            characters.push(ci);
        }
    }

    // 同字 - other lexemes sharing the backbone key, each labelled cognate / false-friend
    let mut same_form = Vec::new();
    let mut same_form_ids = std::collections::HashSet::new();
    for &other in st.graph.lexemes_by_key(&primary) {
        if other == id {
            continue;
        }
        same_form_ids.insert(other);
        let relation = classify_relation(conn, id, other)?;
        if let Some(l) = link_lite(conn, other, relation, None)? {
            same_form.push(l);
        }
        if same_form.len() >= 25 {
            break;
        }
    }

    // 同義 - lexemes sharing a concept (different word, same meaning), excluding same-form ones
    let mut translations = Vec::new();
    let mut seen = same_form_ids;
    seen.insert(id);

    // explicit equivalence edges first (colloquial-Cantonese → standard-Chinese 冇→沒有; curated
    // cross-language 機場→空港). These are precise lexicographer/curated statements, so they lead the
    // "written differently" bridge and are never crowded out by the fuzzy gloss-pivot synonyms below.
    let mut eq = conn.prepare(
        "SELECT CASE WHEN src_lexeme_id=?1 THEN dst_lexeme_id ELSE src_lexeme_id END \
         FROM lexeme_equivalent WHERE src_lexeme_id=?1 OR dst_lexeme_id=?1",
    )?;
    let eq_ids: Vec<i64> = eq.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    for other in eq_ids {
        if !seen.insert(other) {
            continue;
        }
        if let Some(l) = link_lite(conn, other, "equivalent", None)? {
            translations.push(l);
        }
    }

    // prefer tight (specific) concepts; skip hopelessly generic ones to cut polysemy noise
    let mut s = conn.prepare(
        "SELECT s2.lexeme_id, co.label_en, MIN(co.member_count) AS spec \
         FROM sense_concept sc1 \
         JOIN sense_concept sc2 ON sc2.concept_id = sc1.concept_id \
         JOIN sense s1 ON s1.id = sc1.sense_id \
         JOIN sense s2 ON s2.id = sc2.sense_id \
         JOIN concept co ON co.id = sc1.concept_id \
         WHERE s1.lexeme_id = ?1 AND s2.lexeme_id <> ?1 AND co.member_count <= 18 \
         GROUP BY s2.lexeme_id \
         ORDER BY spec ASC \
         LIMIT 120",
    )?;
    let rows: Vec<(i64, String)> =
        s.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    for (other, concept) in rows {
        if !seen.insert(other) {
            continue;
        }
        if let Some(l) = link_lite(conn, other, "synonym", Some(concept))? {
            translations.push(l);
        }
        if translations.len() >= 30 {
            break;
        }
    }

    // 熟語 - for a single character, the common words that contain it (across all systems).
    let mut compounds = Vec::new();
    if headword.chars().count() == 1 {
        if let Some(ch) = headword.chars().next() {
            compounds = char_compounds(conn, ch, &variety, id)?;
        }
    }

    // lexical "why": origin badges + Wiktionary etymology passthrough
    let mut bs = conn.prepare("SELECT badge FROM origin_badge WHERE lexeme_id=?1 ORDER BY badge")?;
    let origin_badges: Vec<String> = bs.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let etymology: Option<String> = conn
        .query_row("SELECT text FROM etymology WHERE lexeme_id=?1", [id], |r| r.get(0))
        .ok();

    Ok(Some(Entry {
        lexeme_id: id,
        variety,
        headword,
        reading,
        freq,
        forms,
        readings,
        senses,
        characters,
        same_form,
        translations,
        compounds,
        origin_badges,
        etymology,
    }))
}

/// 熟語 - words containing a character, shortest then most-frequent first, the given variety preferred.
fn char_compounds(
    conn: &rusqlite::Connection,
    ch: char,
    variety: &str,
    exclude_id: i64,
) -> rusqlite::Result<Vec<LinkLite>> {
    let mut cs = conn.prepare(
        "SELECT l.id FROM form_char fc JOIN lexeme l ON l.id = fc.lexeme_id \
         WHERE fc.cp = ?1 AND l.id <> ?2 GROUP BY l.id \
         ORDER BY (l.variety = ?3) DESC, MIN(fc.flen) ASC, l.freq IS NULL, l.freq DESC, l.id ASC LIMIT 30",
    )?;
    let ids: Vec<i64> = cs
        .query_map(rusqlite::params![ch as i64, exclude_id, variety], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for cid in ids {
        if let Some(l) = link_lite(conn, cid, "compound", None)? {
            out.push(l);
        }
    }
    Ok(out)
}

/// Character-only entry (kokuji / a character with no word-lexeme): readings, decomposition and the
/// words that use it, synthesised from the character tables. Returns None if the codepoint isn't ours.
fn build_char_entry(conn: &rusqlite::Connection, cp: u32) -> rusqlite::Result<Option<Entry>> {
    let ch = match char::from_u32(cp) {
        Some(c) => c,
        None => return Ok(None),
    };
    let ci = match char_info(conn, ch)? {
        Some(ci) => ci,
        None => return Ok(None),
    };
    let variety = crate::search::char_variety(conn, cp as i64)?.to_string();
    let gloss: Option<String> = conn
        .query_row("SELECT gloss_en FROM character WHERE cp=?1", [cp as i64], |r| r.get(0))
        .ok()
        .flatten();
    let senses: Vec<Sense> = gloss.into_iter().map(|g| Sense { pos: None, gloss_en: g }).collect();
    let compounds = char_compounds(conn, ch, &variety, 0)?;
    Ok(Some(Entry {
        lexeme_id: -(cp as i64),
        variety,
        headword: ch.to_string(),
        reading: None,
        freq: None,
        forms: vec![Form { form: ch.to_string(), script: "other".into(), region: None, is_primary: true }],
        readings: Vec::new(),
        senses,
        characters: vec![ci],
        same_form: Vec::new(),
        translations: Vec::new(),
        compounds,
        origin_badges: Vec::new(),
        etymology: None,
    }))
}

/// Do two lexemes share any concept? (kept for reference; relation now uses gloss disjointness)
#[allow(dead_code)]
fn shares_concept(conn: &rusqlite::Connection, a: i64, b: i64) -> rusqlite::Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT EXISTS( \
           SELECT 1 FROM sense_concept x \
           JOIN sense_concept y ON y.concept_id = x.concept_id \
           JOIN sense sx ON sx.id = x.sense_id \
           JOIN sense sy ON sy.id = y.sense_id \
           WHERE sx.lexeme_id = ?1 AND sy.lexeme_id = ?2)",
        [a, b],
        |r| r.get(0),
    )?;
    Ok(n != 0)
}

/// English words too generic to signal shared meaning (don't count as gloss overlap).
const GLOSS_STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "used", "esp", "etc", "sth", "someone", "something", "one",
    "that", "this", "see", "also", "form", "kind", "type", "thing", "person", "make", "made",
    "way", "part", "abbr", "old", "pron", "var", "from", "into", "out", "off", "not", "any",
    "all", "such", "more", "less", "very", "his", "her", "its", "who", "whom", "way", "are",
];

/// A gloss segment that is a cross-reference, not a meaning - "the Japanese word for company",
/// "Mandarin equivalent: 的", "variant of X", "see also X". Its words must not count as shared
/// meaning, or false friends whose dictionary gloss *describes the other language* slip through
/// (会社: jp "company" vs zh "…the Japanese word for company").
fn is_meta_segment(seg: &str) -> bool {
    const META: &[&str] = &[
        "word for", "term for", "equivalent", "variant of", "used in", "abbr", "see also",
        "japanese", "mandarin", "cantonese", "korean",
    ];
    let s = seg.to_lowercase();
    META.iter().any(|m| s.contains(m))
}

/// Content words from a lexeme's PRIMARY sense only (meta cross-references skipped). The first
/// sense is the discriminator: two same-form words are cognates iff their main meanings overlap.
/// Using only sense 1 avoids both false negatives from rich glosses (愛 shares "love" but has many
/// other senses) and false positives from peripheral senses (大丈夫's archaic "great man", 娘's
/// "young (woman)") that incidentally overlap the other language.
fn gloss_words(conn: &rusqlite::Connection, id: i64) -> rusqlite::Result<std::collections::HashSet<String>> {
    let mut s = conn.prepare("SELECT gloss_en FROM sense WHERE lexeme_id=?1 ORDER BY sense_order LIMIT 1")?;
    let glosses: Vec<String> = s.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut out = std::collections::HashSet::new();
    for g in glosses {
        for seg in g.split(|c| c == ';' || c == ',') {
            if is_meta_segment(seg) {
                continue;
            }
            for tok in seg.to_lowercase().split(|c: char| !c.is_ascii_alphabetic()) {
                if tok.len() >= 3 && !GLOSS_STOPWORDS.contains(&tok) {
                    out.insert(tok.to_string());
                }
            }
        }
    }
    Ok(out)
}

const IDENTITY_TYPES: &str = "('simplification','shinjitai','z-variant')";

/// A lexeme's primary surface form (the string compared char-by-char for variant detection).
fn primary_form(conn: &rusqlite::Connection, id: i64) -> rusqlite::Result<String> {
    conn.query_row(
        "SELECT COALESCE((SELECT form FROM surface_form WHERE lexeme_id=l.id AND is_primary=1 LIMIT 1), headword) \
         FROM lexeme l WHERE id=?1",
        [id],
        |r| r.get(0),
    )
}

/// Are two characters a clean 1:1 variant spelling of the same character? True when an identity
/// edge links them AND the derived form has exactly ONE identity parent - i.e. a plain spelling
/// difference (这↔這, 汉↔漢), NOT a simplification *merge* of distinct characters (发←髮/發,
/// 干←乾/幹), which the multi-parent test deliberately excludes so real merges stay false-friends.
fn clean_variant_chars(conn: &rusqlite::Connection, x: char, y: char) -> rusqlite::Result<bool> {
    if x == y {
        return Ok(true);
    }
    for (child, parent) in [(x, y), (y, x)] {
        let linked: i64 = conn.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM glyph_edge WHERE child_cp=?1 AND parent_cp=?2 AND type IN {IDENTITY_TYPES})"),
            [child as i64, parent as i64],
            |r| r.get(0),
        )?;
        if linked != 0 {
            let parents: i64 = conn.query_row(
                &format!("SELECT COUNT(DISTINCT parent_cp) FROM glyph_edge WHERE child_cp=?1 AND type IN {IDENTITY_TYPES}"),
                [child as i64],
                |r| r.get(0),
            )?;
            if parents == 1 {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Are two lexemes the same word in a different spelling? (equal length, every char a clean variant)
fn variant_spelling(conn: &rusqlite::Connection, a: i64, b: i64) -> rusqlite::Result<bool> {
    let fa = primary_form(conn, a)?;
    let fb = primary_form(conn, b)?;
    // identical spellings are NOT a "variant spelling" - same written form, so meaning (gloss)
    // must decide. This keeps genuine same-form false friends (手紙 letter/toilet paper) flagged.
    if fa == fb {
        return Ok(false);
    }
    let (ca, cb): (Vec<char>, Vec<char>) = (fa.chars().collect(), fb.chars().collect());
    if ca.is_empty() || ca.len() != cb.len() {
        return Ok(false);
    }
    for (x, y) in ca.iter().zip(cb.iter()) {
        if !clean_variant_chars(conn, *x, *y)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Classify the relation between two same-form lexemes. Gloss DISJOINTNESS is the decisive signal:
/// two glossed forms that share no meaning word are a false friend; any shared word (or a mere
/// variant spelling, or one side unglossed) is a cognate. This is more reliable than the concept
/// layer, which both under-links (砂糖 = sugar/sugar) and over-links (大丈夫, 娘 - classic false
/// friends it wrongly tied together). Variant spellings (这/這, 汉/漢) and bare variant glyphs with
/// no glosses stay cognate; genuine same-form divergences (手紙, 汽車, 大丈夫, 娘) flag.
fn classify_relation(conn: &rusqlite::Connection, a: i64, b: i64) -> rusqlite::Result<&'static str> {
    // A variant spelling means "the same word, written differently" - but only WITHIN a language.
    // Across languages, variant-equivalent forms can still be false friends (会社 jp "company" vs
    // 會社 zh "guild" - 会 is just the shinjitai of 會), so only short-circuit same-variety pairs.
    let va: String = conn.query_row("SELECT variety FROM lexeme WHERE id=?1", [a], |r| r.get(0))?;
    let vb: String = conn.query_row("SELECT variety FROM lexeme WHERE id=?1", [b], |r| r.get(0))?;
    if va == vb && variant_spelling(conn, a, b)? {
        return Ok("cognate");
    }
    // primary senses share no word → a false friend (手紙, 汽車, 大丈夫, 娘, 会社); any shared
    // primary-sense word → cognate (砂糖 = sugar, 愛 = love). One side unglossed → cognate.
    let wa = gloss_words(conn, a)?;
    let wb = gloss_words(conn, b)?;
    let diverges = !wa.is_empty() && !wb.is_empty() && wa.is_disjoint(&wb);
    Ok(if diverges { "false-friend" } else { "cognate" })
}

pub async fn why_handler(
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<WhyResponse>, (StatusCode, Json<Value>)> {
    let conn = st.pool.get().map_err(internal)?;
    match build_why(&conn, id).map_err(internal)? {
        Some(w) => Ok(Json(w)),
        None => Err((StatusCode::NOT_FOUND, Json(json!({ "error": "not_found" })))),
    }
}

fn build_why(conn: &rusqlite::Connection, id: i64) -> rusqlite::Result<Option<WhyResponse>> {
    let row = conn.query_row(
        "SELECT headword, COALESCE((SELECT form FROM surface_form WHERE lexeme_id=l.id AND is_primary=1 LIMIT 1), headword) \
         FROM lexeme l WHERE id=?1",
        [id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    );
    let (headword, primary) = match row {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut characters = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for ch in primary.chars() {
        if seen.insert(ch) {
            if let Some(ci) = char_info(conn, ch)? {
                characters.push(ci);
            }
        }
    }
    Ok(Some(WhyResponse { lexeme_id: id, headword, characters }))
}

fn char_info(conn: &rusqlite::Connection, ch: char) -> rusqlite::Result<Option<CharInfo>> {
    let cp = ch as i64;
    let row = conn.query_row(
        "SELECT is_orthodox, strokes, radical, ids, gloss_en FROM character WHERE cp=?1",
        [cp],
        |r| {
            Ok((
                r.get::<_, i64>(0)? != 0,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        },
    );
    let (is_orthodox, strokes, radical, ids, gloss_en) = match row {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e),
    };

    let mut s = conn.prepare("SELECT kind, value FROM char_reading WHERE cp=?1")?;
    let readings: Vec<ReadingKV> = s
        .query_map([cp], |r| Ok(ReadingKV { kind: r.get(0)?, value: r.get(1)? }))?
        .collect::<Result<_, _>>()?;

    // identity edges to orthodox parents (the orthographic "why": chain + which reform produced it)
    let mut s = conn.prepare(
        "SELECT p.char, e.type, e.reform_id, rf.name, rf.year FROM glyph_edge e \
         JOIN character p ON p.cp = e.parent_cp \
         LEFT JOIN reform rf ON rf.id = e.reform_id \
         WHERE e.child_cp = ?1 AND e.type IN ('simplification','shinjitai','z-variant')",
    )?;
    let variants: Vec<VariantEdge> = s
        .query_map([cp], |r| {
            Ok(VariantEdge {
                parent: r.get(0)?,
                edge_type: r.get(1)?,
                reform: r.get(2)?,
                reform_name: r.get(3)?,
                reform_year: r.get(4)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    let script_forms = build_script_forms(conn, cp, ch, is_orthodox)?;

    Ok(Some(CharInfo {
        ch: ch.to_string(),
        is_orthodox,
        strokes,
        radical,
        ids,
        gloss_en,
        readings,
        variants,
        script_forms,
    }))
}

fn script_of(edge_type: &str) -> &'static str {
    match edge_type {
        "simplification" => "simplified",
        "shinjitai" => "shinjitai",
        "z-variant" => "z-variant",
        _ => "variant",
    }
}

/// Plain-language name for the reform that produced a branch (the orthographic "why").
fn reform_label(reform_id: Option<&str>) -> Option<String> {
    let id = reform_id?;
    let label = match id {
        "opencc" | "prc-1956" | "prc-1964" => "PRC simplification",
        "jp-toyo" => "Tōyō shinjitai",
        "jp-joyo" => "Jōyō shinjitai",
        "hk-std" => "HK standard",
        "tw-std" => "TW standard",
        "unihan-variant" => "variant",
        other => return Some(other.to_string()),
    };
    Some(label.to_string())
}

/// The character's script family (繁→简·日) anchored on the orthodox glyph: the orthodox form itself
/// plus its living simplified/shinjitai/z-variant children, each reform-labelled. Returns None when
/// there's nothing to show (no cross-script branches and not a kokuji).
fn build_script_forms(
    conn: &rusqlite::Connection,
    cp: i64,
    ch: char,
    is_orthodox: bool,
) -> rusqlite::Result<Option<ScriptForms>> {
    use rusqlite::OptionalExtension;
    // resolve the orthodox anchor of this character's family
    let (anchor_cp, anchor_char): (i64, String) = if is_orthodox {
        (cp, ch.to_string())
    } else {
        conn.query_row(
            &format!(
                "SELECT p.cp, p.char FROM glyph_edge e JOIN character p ON p.cp = e.parent_cp \
                 WHERE e.child_cp = ?1 AND e.type IN {IDENTITY_TYPES} AND p.is_orthodox = 1 LIMIT 1"
            ),
            [cp],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or((cp, ch.to_string()))
    };

    // children of the anchor, merged per glyph (一字 can be BOTH a PRC-simp and a JP-shinjitai of the
    // same orthodox char, e.g. 学←學: show one 学 branch carrying both reform labels)
    let mut s = conn.prepare(&format!(
        "SELECT c.char, c.is_orthodox, e.type, e.reform_id FROM glyph_edge e \
         JOIN character c ON c.cp = e.child_cp \
         WHERE e.parent_cp = ?1 AND e.type IN {IDENTITY_TYPES}"
    ))?;
    let rows: Vec<(String, bool, String, Option<String>)> = s
        .query_map([anchor_cp], |r| {
            Ok((r.get(0)?, r.get::<_, i64>(1)? != 0, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<_, _>>()?;

    let mut order: Vec<String> = Vec::new();
    let mut merged: std::collections::HashMap<String, (Vec<String>, Vec<String>, bool)> =
        std::collections::HashMap::new();
    for (form, child_orth, etype, rid) in rows {
        let e = merged.entry(form.clone()).or_insert_with(|| {
            order.push(form.clone());
            (Vec::new(), Vec::new(), child_orth)
        });
        let sc = script_of(&etype).to_string();
        if !e.0.contains(&sc) {
            e.0.push(sc);
        }
        if let Some(lbl) = reform_label(rid.as_deref()) {
            if !e.1.contains(&lbl) {
                e.1.push(lbl);
            }
        }
    }

    let mut branches = vec![FormBranch {
        form: anchor_char.clone(),
        script: "traditional".into(),
        reform_id: None,
        reform_label: None,
        is_orthodox: true,
    }];
    for form in order {
        let (scripts, labels, child_orth) = merged.remove(&form).unwrap();
        branches.push(FormBranch {
            form,
            script: scripts.join("+"),
            reform_id: None,
            reform_label: if labels.is_empty() { None } else { Some(labels.join(" · ")) },
            is_orthodox: child_orth,
        });
    }

    // kokuji ("Japanese-coined, no Chinese form"): orthodox, no identity edges, has Japanese
    // readings, and NO Chinese (zh/yue) word uses the glyph. The "no zh/yue lexeme" test is what
    // separates pure kokuji (峠 辻 凪 榊) from kokuji reborrowed into Chinese (働 腺 畑) — Unihan's
    // nominal pinyin can't distinguish them.
    let has_edge: i64 = conn.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM glyph_edge WHERE (child_cp=?1 OR parent_cp=?1) AND type IN {IDENTITY_TYPES})"),
        [anchor_cp],
        |r| r.get(0),
    )?;
    let has_kana: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM char_reading WHERE cp=?1 AND kind IN ('onyomi','kunyomi'))",
        [anchor_cp],
        |r| r.get(0),
    )?;
    let chinese_uses: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM surface_form sf JOIN lexeme l ON l.id=sf.lexeme_id \
         WHERE sf.form=?1 AND l.variety IN ('zh','yue'))",
        [&anchor_char],
        |r| r.get(0),
    )?;
    let is_kokuji = is_orthodox && has_edge == 0 && has_kana != 0 && chinese_uses == 0;

    if branches.len() <= 1 && !is_kokuji {
        return Ok(None);
    }
    Ok(Some(ScriptForms { orthodox: anchor_char, is_kokuji, branches }))
}

fn link_lite(
    conn: &rusqlite::Connection,
    id: i64,
    relation: &str,
    concept: Option<String>,
) -> rusqlite::Result<Option<LinkLite>> {
    let row = conn.query_row(
        "SELECT variety, headword, reading FROM lexeme WHERE id=?1",
        [id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)),
    );
    let (variety, headword, reading) = match row {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut s = conn.prepare("SELECT gloss_en FROM sense WHERE lexeme_id=?1 ORDER BY sense_order LIMIT 3")?;
    let glosses: Vec<String> = s.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(Some(LinkLite { lexeme_id: id, variety, headword, reading, glosses, relation: relation.to_string(), concept }))
}

#[derive(Deserialize)]
pub struct TranslateParams {
    pub q: String,
}

/// English-pivot translation: term → concepts → equivalents across all four systems.
pub async fn translate_handler(
    State(st): State<AppState>,
    Query(p): Query<TranslateParams>,
) -> Result<Json<TranslateResponse>, (StatusCode, Json<Value>)> {
    let conn = st.pool.get().map_err(internal)?;
    let resp = build_translate(&conn, &p.q).map_err(internal)?;
    Ok(Json(resp))
}

fn build_translate(conn: &rusqlite::Connection, q: &str) -> rusqlite::Result<TranslateResponse> {
    let term = q.trim().to_lowercase();
    // concepts whose label matches the term (exact label is the gloss-pivot key)
    let mut s = conn.prepare("SELECT id, label_en FROM concept WHERE label_en = ?1 LIMIT 8")?;
    let concepts: Vec<(i64, String)> =
        s.query_map([&term], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;

    let mut groups = Vec::new();
    for (cid, label) in concepts {
        let mut ms = conn.prepare(
            "SELECT DISTINCT s.lexeme_id FROM sense_concept sc \
             JOIN sense s ON s.id = sc.sense_id WHERE sc.concept_id = ?1 LIMIT 40",
        )?;
        let ids: Vec<i64> = ms.query_map([cid], |r| r.get(0))?.collect::<Result<_, _>>()?;
        let mut members = Vec::new();
        for id in ids {
            if let Some(l) = link_lite(conn, id, "synonym", Some(label.clone()))? {
                members.push(l);
            }
        }
        // order by variety so all systems are visible together
        members.sort_by(|a, b| a.variety.cmp(&b.variety));
        groups.push(ConceptGroup { concept: label, members });
    }
    Ok(TranslateResponse { query: q.to_string(), concepts: groups })
}

fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, Json<Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
}
