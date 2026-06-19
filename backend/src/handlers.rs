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

#[derive(Deserialize)]
pub struct SuggestParams {
    pub q: String,
    pub limit: Option<usize>,
}

pub async fn suggest_handler(
    State(st): State<AppState>,
    Query(p): Query<SuggestParams>,
) -> Result<Json<SuggestResponse>, (StatusCode, Json<Value>)> {
    let conn = st.pool.get().map_err(internal)?;
    let limit = p.limit.unwrap_or(8).clamp(1, 20);
    let resp = search::suggest(&conn, &p.q, limit).map_err(internal)?;
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

    // "everyday word": for a single character, the natural MULTI-character word a Chinese / Cantonese
    // speaker would actually write for this character's primary meaning (耳 → 耳朵, 朵 → 花朵). A
    // Japanese learner sees 耳's bare Chinese gloss but wouldn't know 耳朵 is how it's really said.
    // Derived, not curated: the candidate must (a) share the character's primary-sense concept, (b)
    // CONTAIN the character (form_char) — which kills loose synonyms — and (c) be MORE frequent than
    // the bare character in that same language, so we never suggest a compound when the character is
    // itself the everyday word (山, 人) and never surface a mere near-synonym (mountain→小山 "hill").
    if headword.chars().count() == 1 {
        if let Some(cp) = headword.chars().next().map(|c| c as i64) {
            let mut ew = conn.prepare(
                "SELECT m.id, m.variety, m.freq, \
                        (SELECT b.freq FROM lexeme b WHERE b.headword=?3 AND b.variety=m.variety) AS bare \
                 FROM sense_concept sc1 \
                 JOIN sense s1 ON s1.id=sc1.sense_id AND s1.lexeme_id=?1 AND s1.sense_order=0 \
                 JOIN concept co ON co.id=sc1.concept_id AND co.member_count<=18 \
                 JOIN sense_concept sc2 ON sc2.concept_id=sc1.concept_id \
                 JOIN sense s2 ON s2.id=sc2.sense_id AND s2.sense_order=0 \
                 JOIN lexeme m ON m.id=s2.lexeme_id AND m.variety IN ('zh','yue') AND m.freq IS NOT NULL \
                 JOIN form_char fc ON fc.lexeme_id=m.id AND fc.cp=?2 AND fc.flen>=2 \
                 GROUP BY m.id ORDER BY m.variety, m.freq DESC",
            )?;
            let rows: Vec<(i64, String, f64, Option<f64>)> = ew
                .query_map(rusqlite::params![id, cp, headword], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })?
                .collect::<Result<_, _>>()?;
            let mut taken: std::collections::HashSet<String> = std::collections::HashSet::new();
            for (other, var, freq, bare) in rows {
                // strictly more frequent than the bare character in that language; one per variety
                let beats_bare = matches!(bare, Some(b) if freq > b);
                if !beats_bare || taken.contains(&var) {
                    continue;
                }
                taken.insert(var);
                if !seen.insert(other) {
                    continue;
                }
                if let Some(l) = link_lite(conn, other, "everyday-word", None)? {
                    translations.push(l);
                }
            }
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
           AND s1.sense_order = 0 AND s2.sense_order = 0 \
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
            compounds = char_compounds(conn, ch, id)?;
        }
    }

    // lexical "why": origin badges + Wiktionary etymology passthrough
    let mut bs = conn.prepare("SELECT badge FROM origin_badge WHERE lexeme_id=?1 ORDER BY badge")?;
    let origin_badges: Vec<String> = bs.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let etymology: Option<String> = etymology_of(conn, id);

    // per-language origin accounts: the looked-up lexeme first, then the same-glyph cognates in the
    // OTHER languages (山 carries both a Sinitic and a Japonic etymology). One account per variety.
    let mut origins: Vec<OriginAccount> = Vec::new();
    let mut ety_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(t) = etymology.clone() {
        ety_vars.insert(variety.clone());
        origins.push(origin_account(conn, &variety, &headword, t));
    }
    for l in &same_form {
        if ety_vars.contains(&l.variety) {
            continue;
        }
        if let Some(t) = etymology_of(conn, l.lexeme_id) {
            ety_vars.insert(l.variety.clone());
            origins.push(origin_account(conn, &l.variety, &l.headword, t));
        }
    }

    // "appears in characters" replaces the word "used in" for a radical/bound component AND for a
    // glossless single-glyph component (𦘒, 肀): a character with no senses of its own is still worth
    // showing as "only used inside these characters" instead of a blank page.
    let single_char = headword.chars().count() == 1;
    let is_radical_char = characters.first().map(|c| c.is_radical).unwrap_or(false);
    let appears_in = if single_char && (is_radical_char || senses.is_empty()) {
        appears_in_chars(conn, headword.chars().next().unwrap())?
    } else {
        Vec::new()
    };

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
        origins,
        appears_in,
    }))
}

/// Identity-variant glyphs of a character in either direction (the SAME character in another script:
/// 冰 ⇄ 氷, 漢 ⇄ 汉). Used to find words "written differently" with the cross-script form.
fn variant_glyphs(conn: &rusqlite::Connection, ch: char) -> Vec<char> {
    let cp = ch as i64;
    let sql = format!(
        "SELECT p.cp FROM glyph_edge e JOIN character p ON p.cp = e.parent_cp \
           WHERE e.child_cp = ?1 AND e.type IN {IDENTITY_TYPES} \
         UNION SELECT c.cp FROM glyph_edge e JOIN character c ON c.cp = e.child_cp \
           WHERE e.parent_cp = ?1 AND e.type IN {IDENTITY_TYPES}"
    );
    let mut out = Vec::new();
    if let Ok(mut s) = conn.prepare(&sql) {
        if let Ok(rows) = s.query_map([cp], |r| r.get::<_, i64>(0)) {
            for r in rows.flatten() {
                if let Some(c) = char::from_u32(r as u32) {
                    if c != ch {
                        out.push(c);
                    }
                }
            }
        }
    }
    out
}

/// One compound row, displaying the surface form that uses the EXACT looked-up glyph (種馬 on a 馬
/// page, 种马 on a 马 page) and falling back to the lexeme headword. The top/bottom split is decided by
/// CONTENT: a row is "compound" (top) when its displayed form contains the exact glyph `ch`, else
/// "compound-alt" (it only uses a cross-script variant like 马). Note: there is no per-surface-form
/// frequency in the data, so we can't rank 馬上-vs-马上 "by which is used more"; instead the page
/// consistently shows the form written with the character you're viewing (item 160).
fn link_lite_compound(
    conn: &rusqlite::Connection,
    id: i64,
    ch: char,
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
    let mut fs = conn.prepare("SELECT form FROM surface_form WHERE lexeme_id=?1")?;
    let forms: Vec<String> = fs.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let display = forms.iter().find(|f| f.contains(ch)).cloned().unwrap_or(headword);
    let relation = if display.contains(ch) { "compound" } else { "compound-alt" };
    let mut s = conn.prepare("SELECT gloss_en FROM sense WHERE lexeme_id=?1 ORDER BY sense_order LIMIT 3")?;
    let glosses: Vec<String> = s.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(Some(LinkLite {
        lexeme_id: id,
        variety,
        headword: display,
        reading,
        glosses,
        relation: relation.to_string(),
        concept: None,
    }))
}

/// 熟語 - words that contain a character. The exact glyph AND its cross-script variants (马 for 馬) are
/// gathered in ONE frequency-ranked query, deduped per lexeme, then each row is classified top/bottom
/// by whether its displayed form actually contains the exact glyph (so 種馬, which uses 馬, is never
/// banished to the "written differently" group just because of a result-cap race). The language is
/// shown per row, so there's no per-variety sectioning.
fn char_compounds(
    conn: &rusqlite::Connection,
    ch: char,
    exclude_id: i64,
) -> rusqlite::Result<Vec<LinkLite>> {
    let mut cps: Vec<i64> = vec![ch as i64];
    cps.extend(variant_glyphs(conn, ch).iter().map(|c| *c as i64));
    let ph = cps.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT l.id FROM form_char fc JOIN lexeme l ON l.id = fc.lexeme_id \
         WHERE fc.cp IN ({ph}) AND l.id <> ? GROUP BY l.id \
         ORDER BY l.freq IS NULL, l.freq DESC, MIN(fc.flen) ASC, l.id ASC LIMIT 60"
    );
    let mut params: Vec<rusqlite::types::Value> =
        cps.iter().map(|c| rusqlite::types::Value::from(*c)).collect();
    params.push(rusqlite::types::Value::from(exclude_id));
    let mut cs = conn.prepare(&sql)?;
    let ids: Vec<i64> = cs
        .query_map(rusqlite::params_from_iter(params.iter()), |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    // content-based split: same-glyph rows first (freq order kept), variant rows after.
    let mut top = Vec::new();
    let mut alt = Vec::new();
    for id in ids {
        if let Some(l) = link_lite_compound(conn, id, ch)? {
            if l.relation == "compound" {
                top.push(l);
            } else {
                alt.push(l);
            }
        }
    }
    top.extend(alt);
    Ok(top)
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
    let compounds = char_compounds(conn, ch, 0)?;
    let is_radical = ci.is_radical;

    // per-language origin accounts from ANY word-lexeme written with this glyph, so the char page is
    // not thin (and matches the word page): one etymology per variety.
    let mut origins: Vec<OriginAccount> = Vec::new();
    let mut ety_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut q = conn.prepare("SELECT id, variety, headword FROM lexeme WHERE headword=?1")?;
    let cands: Vec<(i64, String, String)> = q
        .query_map([ch.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    for (lid, var, hw) in cands {
        if ety_vars.contains(&var) {
            continue;
        }
        if let Some(t) = etymology_of(conn, lid) {
            ety_vars.insert(var.clone());
            origins.push(origin_account(conn, &var, &hw, t));
        }
    }
    let etymology = origins.first().map(|o| o.text.clone());
    // radical OR glossless component (𦘒): show the characters it appears inside rather than nothing.
    let appears_in = if is_radical || senses.is_empty() { appears_in_chars(conn, ch)? } else { Vec::new() };

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
        etymology,
        origins,
        appears_in,
    }))
}

/// Do two lexemes share a concept (a normalised-gloss synonym cluster)? A shared concept means at
/// least one sense of each carries the SAME meaning, even when their primary glosses differ in
/// wording or sense order. Used in classify_relation to rescue genuine cognates the primary-gloss
/// test wrongly splits when the zh and ja dictionaries lead with different senses (天 = sky/heaven in
/// both, but zh lists "day" first; 本 = root/book in both). It only ever turns a candidate
/// false-friend INTO a cognate, so it cannot mislabel a real false friend (手紙, 娘, 会社 share none).
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
/// "young (woman)") that incidentally overlap the other language. When the two dictionaries order
/// their senses differently the primary glosses can wrongly look disjoint (天 zh "day" vs ja "sky",
/// though both mean sky/heaven) - that gap is closed by the shared-concept check in classify_relation,
/// not by widening this window (which would re-introduce the peripheral-sense false negatives above).
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
    // …but a shared concept (a non-primary sense that means the same thing) overrides a disjoint
    // primary gloss, so sense-ordering differences don't fake a false friend (天, 本).
    if diverges && shares_concept(conn, a, b)? {
        return Ok("cognate");
    }
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

fn is_han_component(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x9FFF | 0xF900..=0xFAFF) || c == '々'
}
fn ids_of(conn: &rusqlite::Connection, ch: char) -> Option<String> {
    conn.query_row("SELECT ids FROM character WHERE cp=?1", [ch as i64], |r| {
        r.get::<_, Option<String>>(0)
    })
    .ok()
    .flatten()
}
/// Han leaf components of an IDS string — drop bracketed source tags ([GTV]), IDC operators and
/// strokes, and the character itself (guards self-referential ids like "木").
fn han_leaves(ids: &str, self_ch: char) -> Vec<char> {
    let mut out = Vec::new();
    let mut in_tag = false;
    for c in ids.chars() {
        match c {
            '[' => in_tag = true,
            ']' => in_tag = false,
            _ if in_tag => {}
            _ if is_han_component(c) && c != self_ch => out.push(c),
            _ => {}
        }
    }
    out
}
/// A "doubled" character: its IDS is 2+ copies of a single component (林=木木, 沝=水水, 昍=日日).
fn is_pure_repeat(conn: &rusqlite::Connection, ch: char) -> bool {
    match ids_of(conn, ch) {
        Some(ids) => {
            let leaves = han_leaves(&ids, ch);
            leaves.len() >= 2 && leaves.iter().all(|&c| c == leaves[0])
        }
        None => false,
    }
}
/// Flatten a character into its multiset of atoms. Only "pure-repeat" components are expanded
/// (林 → 木 木); every other component is kept whole, so 水 stays 水 instead of exploding into its
/// stroke IDS. 森 (⿱木林) → [木, 木, 木]; 淼 (⿱水沝) → [水, 水, 水]; 好 (⿰女子) → [女, 子].
fn atomize(conn: &rusqlite::Connection, ch: char, depth: u8, out: &mut Vec<char>) {
    if depth > 6 {
        out.push(ch);
        return;
    }
    let leaves = ids_of(conn, ch).map(|ids| han_leaves(&ids, ch)).unwrap_or_default();
    if leaves.is_empty() {
        out.push(ch);
        return;
    }
    for l in leaves {
        if is_pure_repeat(conn, l) {
            atomize(conn, l, depth + 1, out);
        } else {
            out.push(l);
        }
    }
}
/// The Wiktionary etymology paragraph for a lexeme, if any.
fn etymology_of(conn: &rusqlite::Connection, id: i64) -> Option<String> {
    conn.query_row("SELECT text FROM etymology WHERE lexeme_id=?1", [id], |r| r.get(0)).ok()
}

/// A gloss that flags the glyph as a Kangxi radical / bound component ("Kangxi radical 60",
/// "radical number 85", "rad. no. 162").
fn is_radical_gloss(gloss: Option<&str>) -> bool {
    match gloss {
        Some(g) => {
            let l = g.to_lowercase();
            l.contains("kangxi radical") || l.contains("radical number") || l.contains("rad. no")
        }
        None => false,
    }
}

/// Parse the Kangxi radical number out of such a gloss ("...Kangxi radical 60" → 60).
fn radical_gloss_number(gloss: Option<&str>) -> Option<i64> {
    let g = gloss?.to_lowercase();
    for kw in ["kangxi radical", "radical number", "rad. no.", "rad. no", "radical"] {
        if let Some(pos) = g.find(kw) {
            let rest = &g[pos + kw.len()..];
            let num: String = rest
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = num.parse::<i64>() {
                return Some(n);
            }
        }
    }
    None
}

/// Characters that CONTAIN this glyph as a component (氵 → 河, 海, 湖…), via the IDS decomposition —
/// the "appears in characters" list that replaces a radical's word "used in". Lightest (fewest
/// strokes) first; capped so a high-frequency radical doesn't dump thousands.
fn appears_in_chars(conn: &rusqlite::Connection, ch: char) -> rusqlite::Result<Vec<CharLite>> {
    let pat = format!("%{}%", ch);
    // Ordering: common-plane glyphs (cp < U+20000, fonts render them) before rare extension-plane
    // ones (no wall of tofu); within that, traditional/orthodox kanji before simplified so TC and SC
    // forms don't interleave (item 13); then lightest (fewest strokes) first.
    let mut s = conn.prepare(
        "SELECT char, gloss_en, cp FROM character WHERE ids LIKE ?1 AND cp <> ?2 \
         ORDER BY (cp >= 131072), (is_orthodox = 0), strokes IS NULL, strokes ASC, cp ASC LIMIT 40",
    )?;
    let out: Vec<CharLite> = s
        .query_map(rusqlite::params![pat, ch as i64], |r| {
            Ok(CharLite { ch: r.get(0)?, gloss: r.get(1)?, rare: r.get::<_, i64>(2)? >= 0x20000 })
        })?
        .collect::<Result<_, _>>()?;
    Ok(out)
}

/// Which script a glyph belongs to, ONLY when it diverges across reforms: a non-orthodox glyph with
/// an identity parent is "simplified"; an orthodox glyph that has a simplified child is "traditional".
/// Returns None for glyphs identical in every script (山, 古) — there is nothing to disambiguate.
fn glyph_script(conn: &rusqlite::Connection, ch: char) -> Option<String> {
    let cp = ch as i64;
    let is_orthodox: bool = conn
        .query_row("SELECT is_orthodox FROM character WHERE cp=?1", [cp], |r| r.get::<_, i64>(0))
        .ok()
        .map(|v| v != 0)?;
    if !is_orthodox {
        let has_parent: i64 = conn
            .query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM glyph_edge WHERE child_cp=?1 AND type IN {IDENTITY_TYPES})"),
                [cp],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if has_parent != 0 {
            return Some("simplified".into());
        }
    } else {
        let has_child: i64 = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM glyph_edge WHERE parent_cp=?1 AND type='simplification')",
                [cp],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if has_child != 0 {
            return Some("traditional".into());
        }
    }
    None
}

/// Content words of a character's own Unihan gloss (for meaning comparison).
fn char_gloss_words(conn: &rusqlite::Connection, ch: char) -> std::collections::HashSet<String> {
    let g: Option<String> = conn
        .query_row("SELECT gloss_en FROM character WHERE cp=?1", [ch as i64], |r| r.get(0))
        .ok()
        .flatten();
    let mut out = std::collections::HashSet::new();
    if let Some(g) = g {
        for tok in g.to_lowercase().split(|c: char| !c.is_ascii_alphabetic()) {
            if tok.len() >= 3 && !GLOSS_STOPWORDS.contains(&tok) {
                out.insert(tok.to_string());
            }
        }
    }
    out
}

/// When a glyph doubles as the simplified form of one or more DISTINCT characters (丑 = earthly branch
/// AND simplified 醜 "ugly"; 干 ← 乾/幹), a note naming them so the origin paragraph isn't misread as
/// describing the merged-in character. Parents whose meaning overlaps the glyph's own (这↔這, plain
/// spelling variants) are excluded — those aren't merges of distinct characters.
fn merge_note(conn: &rusqlite::Connection, ch: char) -> Option<String> {
    let own = char_gloss_words(conn, ch);
    let mut s = conn
        .prepare(
            "SELECT p.char, p.gloss_en FROM glyph_edge e JOIN character p ON p.cp = e.parent_cp \
             WHERE e.child_cp = ?1 AND e.type = 'simplification'",
        )
        .ok()?;
    let rows: Vec<(String, Option<String>)> = s
        .query_map([ch as i64], |r| Ok((r.get(0)?, r.get(1)?)))
        .ok()?
        .filter_map(Result::ok)
        .collect();
    let mut parts = Vec::new();
    for (pch, pgloss) in rows {
        if pch.chars().next() == Some(ch) {
            continue;
        }
        let pwords: std::collections::HashSet<String> = pgloss
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .split(|c: char| !c.is_ascii_alphabetic())
            .filter(|t| t.len() >= 3 && !GLOSS_STOPWORDS.contains(t))
            .map(|t| t.to_string())
            .collect();
        // a real merge: parent has its own meaning that does NOT overlap this glyph's meaning
        if !pwords.is_empty() && pwords.is_disjoint(&own) {
            let short = pgloss
                .as_deref()
                .map(|g| g.split([';', ',']).next().unwrap_or(g).trim().to_string())
                .filter(|s| !s.is_empty());
            match short {
                Some(g) => parts.push(format!("{pch} ({g})")),
                None => parts.push(pch),
            }
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!("Also the simplified form of {}.", parts.join(", ")))
}

/// Build an OriginAccount, stamping the script and any simplification-merge note for single Chinese /
/// Cantonese glyphs (so the reader can tell simplified from traditional, and spot merged characters).
fn origin_account(
    conn: &rusqlite::Connection,
    variety: &str,
    headword: &str,
    text: String,
) -> OriginAccount {
    let (script, note) = if matches!(variety, "zh" | "yue") && headword.chars().count() == 1 {
        let ch = headword.chars().next().unwrap();
        (glyph_script(conn, ch), merge_note(conn, ch))
    } else {
        (None, None)
    };
    OriginAccount { variety: variety.to_string(), headword: headword.to_string(), text, script, note }
}

/// Radical-variant forms → the parent character whose meaning they carry, so 亻/氵/扌… are glossed
/// as person/water/hand instead of "radical number N" (or, for 亻, nothing at all).
fn radical_parent(c: char) -> char {
    match c {
        '亻' => '人',
        '氵' => '水',
        '扌' => '手',
        '艹' => '艸',
        '灬' => '火',
        '忄' | '㣺' => '心',
        '訁' => '言',
        '糹' | '纟' => '糸',
        '釒' | '钅' => '金',
        '刂' => '刀',
        '辶' => '辵',
        '礻' => '示',
        '衤' => '衣',
        '罒' => '网',
        '冫' => '冰',
        '飠' | '饣' => '食',
        '⺹' => '老',
        _ => c,
    }
}
/// First meaningful English sense of a component, glossing radical-variant forms via their parent.
fn component_gloss(conn: &rusqlite::Connection, ch: char) -> Option<String> {
    let target = radical_parent(ch) as i64;
    conn.query_row("SELECT gloss_en FROM character WHERE cp=?1", [target], |r| {
        r.get::<_, Option<String>>(0)
    })
    .ok()
    .flatten()
}
/// Middle Chinese (廣韻 / Baxter) reading(s) for a character, ordered for stable display.
fn mc_readings(conn: &rusqlite::Connection, ch: char) -> Vec<String> {
    let mut s = match conn
        .prepare("SELECT value FROM char_reading WHERE cp=?1 AND kind='mc' ORDER BY value")
    {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    s.query_map([ch as i64], |r| r.get(0))
        .map(|it| it.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

/// Distinct Han components of a character with their meanings — the "what the parts are" layer of the
/// structure section. Prefers the structured phono-semantic roles (char_component: 媽 = 女 semantic +
/// 馬 phonetic) when present; otherwise falls back to the flat one-level IDS leaves (no role).
/// Order-preserving, deduplicated.
fn char_components(conn: &rusqlite::Connection, ch: char, ids: Option<&str>) -> Vec<Component> {
    // structured roles first (Wiktionary Han-compound)
    if let Ok(mut s) = conn.prepare(
        "SELECT component, role, gloss FROM char_component WHERE cp=?1 ORDER BY ord",
    ) {
        let rows: Vec<(String, Option<String>, Option<String>)> = s
            .query_map([ch as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map(|it| it.filter_map(Result::ok).collect())
            .unwrap_or_default();
        if !rows.is_empty() {
            let mut seen = std::collections::HashSet::new();
            let mut out = Vec::new();
            for (comp, role, gloss) in rows {
                if let Some(c) = comp.chars().next() {
                    if seen.insert(c) {
                        // fall back to the parent-character gloss when the template omitted one
                        let gloss = gloss.or_else(|| component_gloss(conn, c));
                        // a phonetic component lends its own reading as the sound ("(sound: mǎ)")
                        let phonetic = role.as_deref() == Some("phonetic");
                        let sound = if phonetic {
                            conn.query_row(
                                "SELECT value FROM char_reading WHERE cp=?1 AND kind='pinyin' LIMIT 1",
                                [c as i64],
                                |r| r.get(0),
                            )
                            .ok()
                        } else {
                            None
                        };
                        // its Middle Chinese reading(s): the HISTORICAL sound link (同 → duwng)
                        let mc_sound = if phonetic { mc_readings(conn, c) } else { Vec::new() };
                        out.push(Component { ch: comp, gloss, role, sound, mc_sound });
                    }
                }
            }
            return out;
        }
    }
    // fallback: flat IDS leaves, no role information
    let leaves = ids.map(|s| han_leaves(s, ch)).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for c in leaves {
        if seen.insert(c) {
            out.push(Component { ch: c.to_string(), gloss: component_gloss(conn, c), role: None, sound: None, mc_sound: Vec::new() });
        }
    }
    out
}

/// When a character reduces to N≥2 copies of ONE base glyph, name it (森 → 木 ×3). Else None.
fn uniform_decomp(conn: &rusqlite::Connection, ch: char) -> Option<CharDecomp> {
    let mut atoms = Vec::new();
    atomize(conn, ch, 0, &mut atoms);
    if atoms.len() >= 2 && atoms.iter().all(|&c| c == atoms[0]) && atoms[0] != ch {
        Some(CharDecomp { base: atoms[0].to_string(), count: atoms.len() as i64 })
    } else {
        None
    }
}

fn char_info(conn: &rusqlite::Connection, ch: char) -> rusqlite::Result<Option<CharInfo>> {
    let cp = ch as i64;
    let row = conn.query_row(
        "SELECT is_orthodox, strokes, radical, ids, gloss_en, gloss_ja FROM character WHERE cp=?1",
        [cp],
        |r| {
            Ok((
                r.get::<_, i64>(0)? != 0,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        },
    );
    let (is_orthodox, strokes, radical, ids, gloss_en, gloss_ja) = match row {
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
    let decomp = uniform_decomp(conn, ch);
    let components = char_components(conn, ch, ids.as_deref());

    // usage signal + radical detection — both keyed on how many lexemes contain this glyph.
    let used_count: i64 =
        conn.query_row("SELECT count(*) FROM form_char WHERE cp=?1", [cp], |r| r.get(0)).unwrap_or(0);
    // per-language counts (for a language-specific "rarely used" tag): 巴 = zh many, ja few.
    let mut used_by_variety: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    if let Ok(mut s) = conn.prepare(
        "SELECT l.variety, count(*) FROM form_char fc JOIN lexeme l ON l.id = fc.lexeme_id \
         WHERE fc.cp = ?1 GROUP BY l.variety",
    ) {
        if let Ok(rows) = s.query_map([cp], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) {
            for row in rows.flatten() {
                used_by_variety.insert(row.0, row.1);
            }
        }
    }
    // per-language MAX word-frequency among words containing this glyph — the real rarity signal
    // (a type count mislabels common particles like 嗎/也). Drives the "rarely used" tag.
    let mut freq_by_variety: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    if let Ok(mut s) = conn.prepare(
        "SELECT l.variety, MAX(l.freq) FROM form_char fc JOIN lexeme l ON l.id = fc.lexeme_id \
         WHERE fc.cp = ?1 AND l.freq IS NOT NULL GROUP BY l.variety",
    ) {
        if let Ok(rows) = s.query_map([cp], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))) {
            for row in rows.flatten() {
                freq_by_variety.insert(row.0, row.1);
            }
        }
    }
    let rad_gloss = is_radical_gloss(gloss_en.as_deref());
    // a genuine bound radical flags as a radical in its gloss AND appears in almost no words of its
    // own (彳: 3, 辵: 0). 山/木/水 carry a radical gloss too but head thousands of words → not radicals.
    let is_radical = rad_gloss && used_count <= 3;
    let radical_number = if rad_gloss {
        radical_gloss_number(gloss_en.as_deref()).or(radical)
    } else {
        radical
    };
    let parent = radical_parent(ch);
    let standalone = if is_radical && parent != ch { Some(parent.to_string()) } else { None };

    Ok(Some(CharInfo {
        ch: ch.to_string(),
        is_orthodox,
        strokes,
        radical,
        ids,
        gloss_en,
        gloss_ja,
        readings,
        variants,
        script_forms,
        decomp,
        components,
        is_radical,
        radical_number,
        standalone,
        used_count,
        used_by_variety,
        freq_by_variety,
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
