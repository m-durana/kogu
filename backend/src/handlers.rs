//! Axum handlers for the read-only API.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa::IntoParams;

use crate::model::*;
use crate::search;
use crate::state::AppState;

/// Liveness probe: service name and version.
#[utoipa::path(
    get, path = "/health", tag = "meta",
    responses((status = 200, description = "Service is up", body = HealthResponse))
)]
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "kogu", "version": env!("CARGO_PKG_VERSION") }))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchParams {
    /// search text: Han (any script), kana, a romanized reading (romaji / pinyin / jyutping), or English
    pub q: String,
    /// preferred Han script for the displayed form: "trad" or "simp" (also "kana", "shinjitai")
    pub script: Option<String>,
    /// force how a romanized/ambiguous query is read: "sound" (phonetic only), "meaning" (English
    /// gloss only), or unset/"auto" to blend both
    pub scope: Option<String>,
    /// restrict results to one language: "zh", "yue" or "ja"; unset/"all" returns every language
    pub lang: Option<String>,
    /// maximum number of hits (default 50, clamped to 1..=200)
    pub limit: Option<usize>,
}

/// Dictionary search across Mandarin (zh), Cantonese (yue) and Japanese (ja).
///
/// The query is classified (Han / kana / reading / English) and matched against surface forms,
/// readings and English glosses; results are ranked hits with per-hit match type and score.
#[utoipa::path(
    get, path = "/search", tag = "dictionary",
    params(SearchParams),
    responses(
        (status = 200, description = "Ranked hits across all three languages", body = SearchResponse),
        (status = 500, description = "Internal error", body = ApiError),
    )
)]
pub async fn search_handler(
    State(st): State<AppState>,
    Query(p): Query<SearchParams>,
) -> Result<Json<SearchResponse>, (StatusCode, Json<Value>)> {
    let conn = st.pool.get().map_err(internal)?;
    let limit = p.limit.unwrap_or(50).clamp(1, 200);
    let scope = search::Scope::from_param(p.scope.as_deref());
    // language filter: only zh/yue/ja restrict; anything else (incl. "all") means no filter
    let lang = p.lang.as_deref().filter(|l| matches!(*l, "zh" | "yue" | "ja"));
    let resp =
        search::search(&st, &conn, &p.q, p.script.as_deref(), scope, lang, limit).map_err(internal)?;
    Ok(Json(resp))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SuggestParams {
    /// the typed prefix to complete
    pub q: String,
    /// maximum number of suggestions (default 8, clamped to 1..=20)
    pub limit: Option<usize>,
}

/// Lightweight autocomplete: headword + reading candidates for a typed prefix (no senses).
#[utoipa::path(
    get, path = "/suggest", tag = "dictionary",
    params(SuggestParams),
    responses(
        (status = 200, description = "Autocomplete candidates", body = SuggestResponse),
        (status = 500, description = "Internal error", body = ApiError),
    )
)]
pub async fn suggest_handler(
    State(st): State<AppState>,
    Query(p): Query<SuggestParams>,
) -> Result<Json<SuggestResponse>, (StatusCode, Json<Value>)> {
    let conn = st.pool.get().map_err(internal)?;
    let limit = p.limit.unwrap_or(8).clamp(1, 20);
    let resp = search::suggest(&conn, &p.q, limit).map_err(internal)?;
    Ok(Json(resp))
}

/// Full dictionary entry for one lexeme.
///
/// Includes surface forms, readings (with Japanese pitch accent and Cantonese jyutping where
/// known), senses, per-character breakdowns, same-form cognates / false friends, cross-language
/// synonyms, compounds and etymology. A negative id addresses a character-only entry keyed by
/// codepoint (id = -codepoint). Get ids from /search or /suggest.
#[utoipa::path(
    get, path = "/entry/{id}", tag = "dictionary",
    params(("id" = i64, Path, description = "lexeme id from /search; negative = character entry (-codepoint)")),
    responses(
        (status = 200, description = "The entry", body = Entry),
        (status = 404, description = "No such lexeme", body = ApiError),
        (status = 500, description = "Internal error", body = ApiError),
    )
)]
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

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EntriesParams {
    /// comma-separated lexeme ids (negative = character entry), e.g. `ids=8502,93996,-27700`.
    /// Up to 50 per call; unknown ids are skipped, so the array can be shorter than the request.
    pub ids: String,
}

/// Fetch several entries in one request (a batch of `/entry/{id}`), for tools that resolve many words
/// at once without hammering the per-item endpoint. Order follows the request; unknown ids are omitted.
#[utoipa::path(
    get, path = "/entries", tag = "dictionary",
    params(EntriesParams),
    responses(
        (status = 200, description = "The found entries, in request order", body = [Entry]),
        (status = 400, description = "No valid ids given", body = ApiError),
        (status = 500, description = "Internal error", body = ApiError),
    )
)]
pub async fn entries_handler(
    State(st): State<AppState>,
    Query(p): Query<EntriesParams>,
) -> Result<Json<Vec<Entry>>, (StatusCode, Json<Value>)> {
    let mut ids: Vec<i64> = p.ids.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    ids.dedup();
    ids.truncate(50);
    if ids.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "no valid ids" }))));
    }
    let conn = st.pool.get().map_err(internal)?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(e) = build_entry(&st, &conn, id).map_err(internal)? {
            out.push(e);
        }
    }
    Ok(Json(out))
}

/// A random reasonably-common word: powers the "feeling lucky" dice. Picks a uniform-random variety
/// first (so zh's far larger corpus doesn't swamp the mix) then a uniform-random lexeme within it
/// above a light frequency floor, so you land on a real word rather than an obscure hapax. Returns
/// just the id; the client navigates through the normal entry route.
#[utoipa::path(
    get, path = "/random", tag = "dictionary",
    responses(
        (status = 200, description = "A random common lexeme id", body = RandomResponse),
        (status = 500, description = "Internal error", body = ApiError),
    )
)]
pub async fn random_handler(
    State(st): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let conn = st.pool.get().map_err(internal)?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM lexeme WHERE freq >= 0.4 AND variety = ( \
               SELECT v FROM (SELECT 'zh' AS v UNION ALL SELECT 'yue' UNION ALL SELECT 'ja') \
               ORDER BY RANDOM() LIMIT 1 \
             ) ORDER BY RANDOM() LIMIT 1",
            [],
            |r| r.get(0),
        )
        .map_err(internal)?;
    Ok(Json(json!({ "lexeme_id": id })))
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
        // rare=0: rare/irregular/search-only JMdict forms stay matchable but are not shown as variants
        "SELECT form, script, region, is_primary FROM surface_form WHERE lexeme_id=?1 AND rare=0 ORDER BY is_primary DESC",
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

    // readings (hide internal normalisation forms). `accent` is the Japanese pitch accent on a ja
    // kana reading (Kanjium, CC BY-SA 4.0); NULL for every other reading.
    let mut s = conn.prepare(
        "SELECT kind, value, accent FROM lexeme_reading WHERE lexeme_id=?1 \
         AND kind NOT IN ('pinyin_num','pinyin_plain','jyutping_plain')",
    )?;
    let readings: Vec<ReadingKV> = s
        .query_map([id], |r| Ok(ReadingKV { kind: r.get(0)?, value: r.get(1)?, accent: r.get(2)? }))?
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
        // only Han ideographs are real "component characters": skip kana/okurigana so a word like
        // あずかり知る breaks down to 知, not to り as if it were a character.
        if !search::is_han(ch) {
            continue;
        }
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
    // CONTAIN the character (form_char): which kills loose synonyms: and (c) be MORE frequent than
    // the bare character in that same language, so we never suggest a compound when the character is
    // itself the everyday word (山, 人) and never surface a mere near-synonym (mountain→小山 "hill").
    if headword.chars().count() == 1 {
        if let Some(cp) = headword.chars().next().map(|c| c as i64) {
            let mut ew = conn.prepare(
                "SELECT m.id, m.variety, m.freq, \
                        (SELECT b.freq FROM lexeme b WHERE b.headword=?3 AND b.variety=m.variety) AS bare \
                 FROM sense_concept sc1 \
                 JOIN sense s1 ON s1.id=sc1.sense_id AND s1.lexeme_id=?1 AND s1.sense_order=0 \
                 JOIN concept co ON co.id=sc1.concept_id AND co.member_count<=400 \
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
        // rank exact-gloss / OMW links (confidence 1.0 / 0.7) above content-word token links (0.5) so
        // the looser token relations trail rather than lead: keeps the wider coverage without the noise
        // leading the list.
        "SELECT s2.lexeme_id, co.label_en, MIN(co.member_count) AS spec, MAX(sc2.confidence) AS conf \
         FROM sense_concept sc1 \
         JOIN sense_concept sc2 ON sc2.concept_id = sc1.concept_id \
         JOIN sense s1 ON s1.id = sc1.sense_id \
         JOIN sense s2 ON s2.id = sc2.sense_id \
         JOIN concept co ON co.id = sc1.concept_id \
         WHERE s1.lexeme_id = ?1 AND s2.lexeme_id <> ?1 AND co.member_count <= 40 \
           AND s1.sense_order = 0 AND s2.sense_order = 0 \
         GROUP BY s2.lexeme_id \
         ORDER BY conf DESC, spec ASC \
         LIMIT 120",
    )?;
    let rows: Vec<(i64, String)> =
        s.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    // collect first, then float the CROSS-LANGUAGE equivalents to the top. "Related in meaning" exists
    // to answer "what's this word in the other languages?", so a different-variety match (邪魔 ja →
    // 打岔 zh) should lead over a same-language near-synonym (邪魔 → 妨害 ja) even when the same-language
    // one has a slightly tighter concept. Stable partition preserves the conf/spec order within each side.
    let mut syns = Vec::new();
    for (other, concept) in rows {
        if !seen.insert(other) {
            continue;
        }
        if let Some(l) = link_lite(conn, other, "synonym", Some(concept))? {
            syns.push(l);
        }
    }
    syns.sort_by_key(|l| l.variety == variety); // false (cross-language) sorts before true (same)
    syns.truncate(30usize.saturating_sub(translations.len()));
    translations.extend(syns);

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
    // the page's own language, as it appears in a borrowing note: used to drop a circular cross-language
    // account (a 中 account that only says "borrowed from Japanese" adds nothing on the Japanese page).
    let page_lang = if variety == "ja" { "from japanese" } else { "from chinese" };
    for l in &same_form {
        if ety_vars.contains(&l.variety) {
            continue;
        }
        if let Some(t) = etymology_of(conn, l.lexeme_id) {
            // an OTHER-language account that merely states it was borrowed FROM this page's language is
            // circular here (象棋 zh's ja account "From Chinese …"; 霊長 ja's zh account "borrowed from
            // Japanese …"), so it's skipped; a genuine account (the classical source, a native origin) stays.
            if t.to_lowercase().contains(page_lang) {
                continue;
            }
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
    let accent = crate::model::ja_reading_accent(conn, id, &variety, reading.as_deref());
    let jyut = crate::model::zh_jyutping(conn, id, &variety);
    Ok(Some(LinkLite {
        lexeme_id: id,
        variety,
        headword: display,
        reading,
        accent,
        jyut,
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
         ORDER BY l.freq IS NULL, l.freq DESC, MIN(fc.flen) ASC, l.id ASC LIMIT 300"
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
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InterestingParams {
    /// maximum number of items (default 8, clamped 1..=30)
    pub limit: Option<usize>,
}

/// A fresh-random showcase of noteworthy entries for the homepage: kanji invented in Japan (国字),
/// 日/中 false friends, words coined in Japan and re-borrowed into Chinese, 粵字, simplified merges,
/// and English false friends (katakana that looks English but means something else). Different mix each call.
#[utoipa::path(
    get, path = "/interesting", tag = "dictionary",
    params(InterestingParams),
    responses(
        (status = 200, description = "Random showcase of noteworthy entries", body = InterestingResponse),
        (status = 500, description = "Internal error", body = ApiError),
    )
)]
pub async fn interesting_handler(
    State(st): State<AppState>,
    Query(p): Query<InterestingParams>,
) -> Result<Json<InterestingResponse>, (StatusCode, Json<Value>)> {
    let conn = st.pool.get().map_err(internal)?;
    let limit = p.limit.unwrap_or(8).clamp(1, 30);
    let items = build_interesting(&conn, limit).map_err(internal)?;
    Ok(Json(InterestingResponse { items }))
}

/// A headword that will actually render on a device font: contains Han or kana and no Latin letters
/// (skips fullwidth-Latin junk like ＬＡＭＰ and astral-plane tofu is naturally rare here).
fn renderable_cjk(s: &str) -> bool {
    let has_cjk = s.chars().any(|c| {
        let u = c as u32;
        (0x3040..=0x30FF).contains(&u) || (0x4E00..=0x9FFF).contains(&u) || (0x3400..=0x4DBF).contains(&u)
    });
    has_cjk && !s.chars().any(|c| c.is_ascii_alphabetic())
}

/// Curated, hand-verified genuine false friends: the SAME Han spelling with a sharply different
/// meaning in Chinese and Japanese (手紙 = letter in 日 / toilet paper in 中). The automatic
/// gloss-disjointness classifier (classify_relation, used by the Related list) is too noisy for a
/// show-off homepage: it flags near-synonyms whose English glosses merely differ in wording
/// (掃除 "cleaning" vs "to clean", 自重 "self-respect" vs "conduct oneself with dignity") and even
/// leaves cognates like 先生/結婚/成功 looking divergent. So the showcase samples from this vetted
/// set and prints BOTH live glosses, which can neither mislabel nor go stale. Every entry was
/// verified present in both varieties with a genuinely divergent primary sense.
const FALSE_FRIENDS: &[&str] = &[
    "手紙", "汽車", "勉強", "大丈夫", "高校", "新聞", "工夫", "愛人", "丈夫", "邪魔", "皮肉",
    "迷惑", "一味", "結束", "天井", "人間", "深刻", "老婆", "放心", "約束", "娘", "走", "節目",
    "大家", "留守", "手心", "用意", "石頭",
];

/// Run a 5-column category query (id, variety, headword, reading, gloss), keep renderable rows,
/// tag each with a fixed `why`/`category`, and return up to `want`.
fn simple_cat(
    conn: &rusqlite::Connection,
    sql: &str,
    want: usize,
    category: &str,
    why: &str,
) -> rusqlite::Result<Vec<InterestingItem>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| {
        Ok(InterestingItem {
            lexeme_id: r.get(0)?,
            variety: r.get(1)?,
            headword: r.get(2)?,
            reading: r.get(3)?,
            gloss: r.get(4)?,
            why: why.to_string(),
            category: category.to_string(),
        })
    })?;
    let mut out = Vec::new();
    for it in rows {
        let it = it?;
        if renderable_cjk(&it.headword) {
            out.push(it);
            if out.len() >= want {
                break;
            }
        }
    }
    Ok(out)
}

/// Curated katakana gairaigo that look like an English word but mean something else - the classic
/// wasei-eigo false friends (マンション "mansion" = apartment, スマート "smart" = slim/stylish). Each
/// pair is (Japanese headword, the English word it resembles); the real meaning comes from the live
/// gloss so it can't go stale. Every entry hand-verified present as a ja lexeme with a divergent sense.
const ENGLISH_FALSE_FRIENDS: &[(&str, &str)] = &[
    ("マンション", "mansion"),
    ("スマート", "smart"),
    ("テンション", "tension"),
    ("ナイーブ", "naive"),
    ("カンニング", "cunning"),
    ("コンセント", "consent"),
    ("パンツ", "pants"),
    ("ビニール", "vinyl"),
    ("トランプ", "trump"),
    ("ストーブ", "stove"),
    ("クーラー", "cooler"),
    ("タレント", "talent"),
    ("マニア", "mania"),
    ("リフォーム", "reform"),
];

/// The real, DIVERGENT meaning of an English false friend: the first gloss clause that isn't just the
/// English lookalike restated. JMdict often leads with the source word (スマート = "smart (clothing…);
/// stylish", クレーム = "claim (for compensation); customer complaint…") which would make the caption
/// read "looks like smart · means smart"; skip those and surface the clause that actually differs.
fn divergent_meaning(gloss: &str, eng: &str) -> String {
    for seg in gloss.split(';') {
        let cleaned = search::clean_segment(seg);
        if cleaned.is_empty() {
            continue;
        }
        let clause = cleaned.split(',').next().unwrap_or(&cleaned).trim();
        if clause.eq_ignore_ascii_case(eng) {
            continue;
        }
        return clause.to_string();
    }
    short_gloss(gloss)
}

/// English false friends: katakana loanwords that resemble an English word but diverged in meaning.
/// Shows the Japanese entry with the trap spelled out in the `why` - looks like "mansion" · means
/// apartment - so the row teaches the gotcha. The real meaning is the live gloss (never stale); the
/// English lookalike is curated. A distinctly Japan-side, English-learner-friendly curiosity.
fn english_false_friend_items(
    conn: &rusqlite::Connection,
    want: usize,
) -> rusqlite::Result<Vec<InterestingItem>> {
    let ph = ENGLISH_FALSE_FRIENDS.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT l.id, l.headword, l.reading, \
           (SELECT gloss_en FROM sense WHERE lexeme_id=l.id ORDER BY sense_order LIMIT 1) \
         FROM lexeme l WHERE l.variety='ja' AND l.freq IS NOT NULL AND l.headword IN ({ph}) \
         ORDER BY RANDOM()"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(ENGLISH_FALSE_FRIENDS.iter().map(|(h, _)| *h)),
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    let mut out = Vec::new();
    for row in rows {
        let (id, headword, reading, gloss) = row?;
        let Some(gloss) = gloss else { continue };
        // the English lookalike for this headword, from the curated pairs
        let Some((_, eng)) = ENGLISH_FALSE_FRIENDS.iter().find(|(h, _)| *h == headword) else {
            continue;
        };
        out.push(InterestingItem {
            lexeme_id: id,
            variety: "ja".to_string(),
            headword,
            reading,
            gloss: None,
            why: format!("looks like English “{eng}” · actually means {}", divergent_meaning(&gloss, eng)),
            category: "english-false-friend".to_string(),
        });
        if out.len() >= want {
            break;
        }
    }
    Ok(out)
}

/// False friends: sample the curated FALSE_FRIENDS set, show the Japanese entry (its kana reading
/// makes the pair concrete), and put the CONTRAST in the `why` from the two live glosses -
/// "日 letter · 中 toilet paper" - so the row teaches both meanings and shows both languages at once.
/// The gloss is left off because the contrast note already carries the meaning.
fn false_friend_items(conn: &rusqlite::Connection, want: usize) -> rusqlite::Result<Vec<InterestingItem>> {
    let ph = FALSE_FRIENDS.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT j.id, j.variety, j.headword, j.reading, \
           (SELECT gloss_en FROM sense WHERE lexeme_id=j.id ORDER BY sense_order LIMIT 1), \
           (SELECT gloss_en FROM sense WHERE lexeme_id=( \
              SELECT id FROM lexeme WHERE headword=j.headword AND variety='zh' AND freq IS NOT NULL LIMIT 1) \
            ORDER BY sense_order LIMIT 1) \
         FROM lexeme j WHERE j.variety='ja' AND j.freq IS NOT NULL AND j.headword IN ({ph}) \
         ORDER BY RANDOM()"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(FALSE_FRIENDS.iter()), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, variety, headword, reading, jg, zg) = row?;
        // both glosses are needed to state the contrast; skip if either side lost its gloss in a rebuild
        let (Some(jg), Some(zg)) = (jg, zg) else { continue };
        if !renderable_cjk(&headword) {
            continue;
        }
        out.push(InterestingItem {
            lexeme_id: id,
            variety,
            headword,
            reading,
            gloss: None,
            why: format!("日 {} · 中 {}", short_gloss(&jg), short_gloss(&zg)),
            category: "false-friend".to_string(),
        });
        if out.len() >= want {
            break;
        }
    }
    Ok(out)
}

/// 粵字: a character written only for Cantonese (冇 "not have", 佢 "he/she", 咁 "so") - no Mandarin
/// or Japanese lexeme shares the glyph. A distinctly Chinese-side curiosity to offset the Japan-heavy
/// categories.
fn cantoji_items(conn: &rusqlite::Connection, want: usize) -> rusqlite::Result<Vec<InterestingItem>> {
    simple_cat(
        conn,
        "SELECT l.id,l.variety,l.headword,l.reading, \
           (SELECT gloss_en FROM sense WHERE lexeme_id=l.id ORDER BY sense_order LIMIT 1) \
         FROM lexeme l WHERE l.variety='yue' AND length(l.headword)=1 AND l.freq IS NOT NULL \
           AND NOT EXISTS (SELECT 1 FROM lexeme o WHERE o.variety IN ('zh','ja') AND o.headword=l.headword) \
           AND unicode(l.headword) NOT IN (SELECT cp FROM char_reading WHERE kind IN ('mc','onyomi','kunyomi')) \
         ORDER BY RANDOM() LIMIT 40",
        want,
        "cantoji",
        "粵字 · written only in Cantonese",
    )
}

/// A single simplified character that absorbed several DISTINCT traditional characters (干 ← 乾/幹,
/// 里 ← 裏/裡, 台 ← 臺/檯/颱): the classic "one simplified form, several original words" merge. The
/// merged-away forms go in the `why`. Proper-noun senses (surnames, place-names) are dropped by
/// keeping only lowercase-pinyin readings - CC-CEDICT capitalises proper-noun pinyin.
fn simplified_merge_items(conn: &rusqlite::Connection, want: usize) -> rusqlite::Result<Vec<InterestingItem>> {
    let mut stmt = conn.prepare(
        "SELECT l.id,l.variety,l.headword,l.reading, \
           (SELECT gloss_en FROM sense WHERE lexeme_id=l.id ORDER BY sense_order LIMIT 1), \
           (SELECT group_concat(char(parent_cp),' · ') FROM glyph_edge \
              WHERE child_cp=unicode(l.headword) AND type='simplification') \
         FROM lexeme l WHERE l.variety='zh' AND length(l.headword)=1 AND l.freq IS NOT NULL \
           AND substr(l.reading,1,1)=lower(substr(l.reading,1,1)) \
           AND (SELECT count(DISTINCT parent_cp) FROM glyph_edge \
                WHERE child_cp=unicode(l.headword) AND type='simplification') >= 2 \
         ORDER BY RANDOM() LIMIT 40",
    )?;
    let rows = stmt.query_map([], |r| {
        let parents: String = r.get(5)?;
        // at most three merged-away forms so the caption stays one line
        let shown = parents.split(" · ").take(3).collect::<Vec<_>>().join(" · ");
        Ok(InterestingItem {
            lexeme_id: r.get(0)?,
            variety: r.get(1)?,
            headword: r.get(2)?,
            reading: r.get(3)?,
            gloss: r.get(4)?,
            why: format!("one simplified form of {shown}"),
            category: "merge".to_string(),
        })
    })?;
    let mut out = Vec::new();
    for it in rows {
        let it = it?;
        if renderable_cjk(&it.headword) {
            out.push(it);
            if out.len() >= want {
                break;
            }
        }
    }
    Ok(out)
}

/// Assemble the homepage showcase: a fresh-random pick from each category, round-robin merged so
/// every category is represented, truncated to `limit`. Six categories (a symmetrical grid), balanced
/// across languages - Japan-side (kokuji, wasei, English false friends), cross-language (日/中 false
/// friends) and China/Cantonese-side (粵字, simplified merges) - rather than Japan-heavy.
/// Fresh random on every call (SQL RANDOM()).
fn build_interesting(conn: &rusqlite::Connection, limit: usize) -> rusqlite::Result<Vec<InterestingItem>> {
    // enough from each bucket to fill `limit` when some categories come up short (div_ceil, min 1)
    const N_BUCKETS: usize = 6;
    let per = ((limit + N_BUCKETS - 1) / N_BUCKETS).max(1);
    let buckets: Vec<Vec<InterestingItem>> = vec![
        simple_cat(
            conn,
            // true kokuji: a Japanese single-char word, kunyomi-bearing, with NO Chinese/Cantonese
            // lexeme of the same glyph AND no identity edge to an orthodox parent (which would make it
            // a mere shinjitai/simplified FORM of an existing character - 徴←徵, 厨←廚 - not invented in Japan)
            // AND no Middle Chinese (廣韻) reading and no on'yomi - either means it's an attested
            // historical Chinese character read into Japanese (靫/狢/筬/鰄 have MC; 姫 has an on'yomi),
            // whereas a true kokuji is native and kun-only (峠/凪/榊).
            "SELECT l.id,l.variety,l.headword,l.reading, \
               (SELECT gloss_en FROM sense WHERE lexeme_id=l.id ORDER BY sense_order LIMIT 1) \
             FROM lexeme l WHERE l.variety='ja' AND length(l.headword)=1 \
               AND unicode(l.headword) IN (SELECT cp FROM char_reading WHERE kind='kunyomi') \
               AND unicode(l.headword) NOT IN (SELECT cp FROM char_reading WHERE kind IN ('mc','onyomi')) \
               AND NOT EXISTS (SELECT 1 FROM lexeme z WHERE z.variety IN ('zh','yue') AND z.headword=l.headword) \
               AND NOT EXISTS (SELECT 1 FROM glyph_edge WHERE child_cp=unicode(l.headword) \
                 AND type IN ('simplification','shinjitai','z-variant')) \
             ORDER BY RANDOM() LIMIT 40",
            per,
            "kokuji",
            "国字 · a kanji invented in Japan",
        )?,
        false_friend_items(conn, per)?,
        simple_cat(
            conn,
            // Chinese side only (the point is that it entered Chinese), and lowercase-pinyin readings
            // to drop the transliterated Japanese place/person names (熊本, 沖繩, 岩倉) in this badge set.
            "SELECT l.id,l.variety,l.headword,l.reading, \
               (SELECT gloss_en FROM sense WHERE lexeme_id=l.id ORDER BY sense_order LIMIT 1) \
             FROM origin_badge ob JOIN lexeme l ON l.id=ob.lexeme_id \
             WHERE l.variety='zh' AND ob.badge IN ('borrowed-from-japanese','wasei-kango') \
               AND substr(l.reading,1,1)=lower(substr(l.reading,1,1)) \
             ORDER BY RANDOM() LIMIT 40",
            per,
            "wasei",
            "和製漢語 · coined in Japan, borrowed into Chinese",
        )?,
        cantoji_items(conn, per)?,
        simplified_merge_items(conn, per)?,
        english_false_friend_items(conn, per)?,
    ];
    // round-robin merge so a small limit still shows a variety of categories
    let mut out = Vec::new();
    for i in 0..per {
        for b in &buckets {
            if let Some(it) = b.get(i) {
                out.push(it.clone());
            }
        }
    }
    out.truncate(limit);
    Ok(out)
}

fn shares_concept(conn: &rusqlite::Connection, a: i64, b: i64) -> rusqlite::Result<bool> {
    // Only STRONG links (confidence >= 1.0 = exact gloss-pivot / OMW) may rescue a cognate; the looser
    // content-word "gloss-token" links (confidence 0.5) widen the Related list but must NOT flip a real
    // false friend into a cognate on an incidental shared word.
    let n: i64 = conn.query_row(
        "SELECT EXISTS( \
           SELECT 1 FROM sense_concept x \
           JOIN sense_concept y ON y.concept_id = x.concept_id \
           JOIN sense sx ON sx.id = x.sense_id \
           JOIN sense sy ON sy.id = y.sense_id \
           WHERE sx.lexeme_id = ?1 AND sy.lexeme_id = ?2 \
             AND x.confidence >= 1.0 AND y.confidence >= 1.0)",
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
    // language / region LABELS: a gloss tag like "(Cantonese) steering wheel" or "(Cant.)" is not a
    // content word, so it must never count toward meaning-overlap (else a region-tagged char gloss
    // that merely restates a sense - 軚 "(Cant.) a steering wheel" vs sense "(Cantonese) steering
    // wheel" - looks novel and gets shown). Paired with is_meta_segment no longer skipping these.
    "cantonese", "mandarin", "japanese", "korean", "chinese", "cant",
];

/// A gloss segment that is a cross-reference, not a meaning - "the Japanese word for company",
/// "Mandarin equivalent: 的", "variant of X", "see also X". Its words must not count as shared
/// meaning, or false friends whose dictionary gloss *describes the other language* slip through
/// (会社: jp "company" vs zh "…the Japanese word for company"). Only POINTER phrases qualify: a bare
/// region tag like "(Cantonese) steering wheel" is a real meaning (the language name is stopworded
/// out instead), so language names are NOT listed here - matching one wiped a whole tagged sense.
fn is_meta_segment(seg: &str) -> bool {
    const META: &[&str] =
        &["word for", "term for", "equivalent", "variant of", "used in", "abbr", "see also"];
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
                    out.insert(stem(tok).to_string());
                }
            }
        }
    }
    Ok(out)
}

/// A light suffix stripper (NOT a full Porter stemmer) so the same meaning written in a different
/// word FORM still counts as a match: 完成 "completion" vs "to complete", 掃除 "cleaning" vs "to
/// clean", 成功 "success" vs "succeed". Without it, cognates whose zh and ja glosses merely inflect
/// the meaning differently look disjoint and get mislabelled false friends. Longest suffix first.
fn stem(t: &str) -> &str {
    for suf in [
        "ations", "ation", "ings", "ing", "ments", "ment", "ness", "ities", "ion", "ies", "ed",
        "es", "ly", "s",
    ] {
        if let Some(base) = t.strip_suffix(suf) {
            if base.len() >= 3 {
                return base;
            }
        }
    }
    t
}

/// Do two stemmed gloss-word sets share meaning? An exact shared token, OR a common word-stem that
/// only differs by a derivational tail (marriage/married, succes(s)/succe(ed)): the shorter must be
/// a >=5-char prefix of the longer. The 5-char floor keeps unrelated short roots (act/actor,
/// part/party) from colliding. Only ever MERGES words, so it can turn a mislabelled false friend
/// back into a cognate but never the reverse.
fn glosses_overlap(a: &std::collections::HashSet<String>, b: &std::collections::HashSet<String>) -> bool {
    if !a.is_disjoint(b) {
        return true;
    }
    for x in a {
        for y in b {
            // the same root inflected differently: a shared prefix of >=5 chars. Covers both a token
            // that IS a prefix (stimul + stimulant) and two siblings off the same root that only share
            // one (stimulus/stimulant, natural/nature, unity/uniformity) - neither of which is a prefix
            // of the other, so the old starts_with test missed them and split real cognates.
            let common = x.chars().zip(y.chars()).take_while(|(cx, cy)| cx == cy).count();
            if common >= 5 {
                return true;
            }
        }
    }
    false
}

/// Is this lexeme a proper noun (place, surname, given name)? CC-CEDICT capitalises the pinyin of
/// proper-noun senses, so a romanised reading that starts with an uppercase ASCII letter marks one.
/// A same-form proper noun (成功 the town vs 成功 "success") is not a pedagogically useful false
/// friend, so classify_relation treats it as a plain same-form sibling instead of flagging it.
fn is_proper_noun(conn: &rusqlite::Connection, id: i64) -> rusqlite::Result<bool> {
    let reading: Option<String> = conn.query_row("SELECT reading FROM lexeme WHERE id=?1", [id], |r| r.get(0))?;
    Ok(reading
        .and_then(|r| r.chars().next())
        .is_some_and(|c| c.is_ascii_uppercase()))
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
///
/// "Share no word" is measured on STEMMED tokens with a prefix-fuzzy match (glosses_overlap), so a
/// cognate whose two glosses merely inflect the meaning differently (完成 "completion"/"to complete",
/// 掃除 "cleaning"/"to clean", 成功 "success"/"succeed") is no longer mistaken for a false friend.
/// Proper-noun same-forms (成功 the Taiwan town, whose zh gloss is a place name) are never flagged.
fn classify_relation(conn: &rusqlite::Connection, a: i64, b: i64) -> rusqlite::Result<&'static str> {
    // A same-form NAME (place, surname) beside a common word is not a teachable false friend - it is
    // just an incidental spelling coincidence - so treat it as a plain same-form sibling.
    if is_proper_noun(conn, a)? || is_proper_noun(conn, b)? {
        return Ok("cognate");
    }
    // A variant spelling means "the same word, written differently" - but only WITHIN a language.
    // Across languages, variant-equivalent forms can still be false friends (会社 jp "company" vs
    // 會社 zh "guild" - 会 is just the shinjitai of 會), so only short-circuit same-variety pairs.
    let va: String = conn.query_row("SELECT variety FROM lexeme WHERE id=?1", [a], |r| r.get(0))?;
    let vb: String = conn.query_row("SELECT variety FROM lexeme WHERE id=?1", [b], |r| r.get(0))?;
    if va == vb && variant_spelling(conn, a, b)? {
        return Ok("cognate");
    }
    // primary senses share no meaning word → a false friend (手紙, 汽車, 大丈夫, 娘, 会社); any shared
    // (or prefix-equivalent) word → cognate (砂糖 = sugar, 完成 = completion/complete). One side unglossed → cognate.
    let wa = gloss_words(conn, a)?;
    let wb = gloss_words(conn, b)?;
    let diverges = !wa.is_empty() && !wb.is_empty() && !glosses_overlap(&wa, &wb);
    // …but a shared concept (a non-primary sense that means the same thing) overrides a disjoint
    // primary gloss, so sense-ordering differences don't fake a false friend (天, 本).
    if diverges && shares_concept(conn, a, b)? {
        return Ok("cognate");
    }
    Ok(if diverges { "false-friend" } else { "cognate" })
}

/// The orthographic and phonological "why" of a word: its characters with components, roles
/// (semantic / phonetic), script-reform variants, and Middle Chinese sound links.
#[utoipa::path(
    get, path = "/why/{id}", tag = "dictionary",
    params(("id" = i64, Path, description = "lexeme id from /search")),
    responses(
        (status = 200, description = "Per-character explanation", body = WhyResponse),
        (status = 404, description = "No such lexeme", body = ApiError),
        (status = 500, description = "Internal error", body = ApiError),
    )
)]
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
/// Han leaf components of an IDS string: drop bracketed source tags ([GTV]), IDC operators and
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

/// Characters that CONTAIN this glyph as a component (氵 → 河, 海, 湖…), via the IDS decomposition :
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
/// Returns None for glyphs identical in every script (山, 古): there is nothing to disambiguate.
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
/// spelling variants) are excluded: those aren't merges of distinct characters.
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
    // pre-baked English machine translation of a non-English etymology (Chinese 出處 / native 詞源·語源),
    // keyed by the etymology text (identical text shares one translation across lexemes).
    let text_en: Option<String> = conn
        .query_row("SELECT text_en FROM etymology WHERE text=?1 AND text_en IS NOT NULL LIMIT 1", [&text], |r| r.get(0))
        .ok();
    OriginAccount { variety: variety.to_string(), headword: headword.to_string(), text, text_en, script, note }
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

/// Distinct Han components of a character with their meanings: the "what the parts are" layer of the
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

    // ORDER BY ord so a polyphonic character's customary reading comes first (ord 0), then the rest;
    // on/kun/mc rows default to ord 0 and fall back to value order, unchanged.
    let mut s = conn.prepare("SELECT kind, value FROM char_reading WHERE cp=?1 ORDER BY kind, ord, value")?;
    let mut readings: Vec<ReadingKV> = s
        .query_map([cp], |r| Ok(ReadingKV { kind: r.get(0)?, value: r.get(1)?, accent: None }))?
        .collect::<Result<_, _>>()?;

    // Japanese pitch accent (Kanjium) for this character's kana on/kun readings: a single-character
    // Japanese WORD (e.g. 箸 = はし) carries its accent on the word's lexeme_reading, not on the
    // character. Surface it onto the matching kun/on reading so a SINGLE-KANJI entry shows the pitch
    // contour too (箸 はし atamadaka vs 橋 はし odaka vs 端 はし heiban), not just multi-kanji words.
    let ch_s = ch.to_string();
    let mut accent_by_kana: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut s = conn.prepare(
        "SELECT lr.value, lr.accent FROM lexeme_reading lr \
         JOIN surface_form sf ON sf.lexeme_id = lr.lexeme_id \
         JOIN lexeme l ON l.id = lr.lexeme_id \
         WHERE l.variety='ja' AND lr.kind='kana' AND lr.accent IS NOT NULL AND sf.form = ?1",
    )?;
    let arows = s.query_map([&ch_s], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in arows {
        let (kana, accent) = row?;
        accent_by_kana.entry(kana).or_insert(accent);
    }
    if !accent_by_kana.is_empty() {
        for rk in readings.iter_mut() {
            if rk.kind != "kunyomi" && rk.kind != "onyomi" {
                continue;
            }
            // kun readings carry okurigana markers (こころざ.す, -がわ); the word kana is plain.
            let key: String = rk.value.chars().filter(|c| *c != '.' && *c != '-').collect();
            if let Some(a) = accent_by_kana.get(&key) {
                rk.accent = Some(a.clone());
            }
        }
    }

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

    // usage signal + radical detection: both keyed on how many lexemes contain this glyph.
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
    // per-language MAX word-frequency among words containing this glyph: the real rarity signal
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
    // confusable look-alikes (Unihan kSpoofingVariant): a visual-confusability note, not identity or
    // meaning. The table may be absent on an older DB, so degrade gracefully if the query fails.
    let mut confusables: Vec<String> = Vec::new();
    if let Ok(mut s) =
        conn.prepare("SELECT confusable_cp FROM char_confusable WHERE cp = ?1 ORDER BY confusable_cp LIMIT 12")
    {
        if let Ok(rows) = s.query_map([cp], |r| r.get::<_, i64>(0)) {
            for cpx in rows.flatten() {
                if let Some(c) = char::from_u32(cpx as u32) {
                    confusables.push(c.to_string());
                }
            }
        }
    }

    let rad_gloss = is_radical_gloss(gloss_en.as_deref());
    // a genuine bound radical flags as a radical in its gloss AND appears in almost no words of its
    // own (彳: 4, 辵: 0). 山/木/水 carry a radical gloss too but head thousands of words → not radicals.
    // The small threshold has slack for the full JMdict (彳 rose from 3 to 4 standalone uses); real
    // characters sit in the hundreds-to-thousands, far above it.
    let is_radical = rad_gloss && used_count <= 8;
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
        confusables,
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
    // ALL orthodox parents. Plural matters twice over: a simplified char can merge several
    // traditional ones (冲 ← 沖+衝, 干 ← 乾+幹+榦), and a char that is itself orthodox can still
    // be a merger target (週/賙 → 周). The old LIMIT-1 pick hid every parent but an arbitrary one.
    // Order: simplification edges first, PRC reforms first, then codepoint: so `orthodox` (the
    // primary anchor, also what the ja-form fallback on the frontend uses) is deterministic.
    let mut ps = conn.prepare(&format!(
        "SELECT p.cp, p.char, \
                MIN(e.type != 'simplification') AS not_simp, \
                MIN(CASE WHEN e.reform_id IN ('opencc','prc-1956','prc-1964') THEN 0 ELSE 1 END) AS not_prc \
         FROM glyph_edge e JOIN character p ON p.cp = e.parent_cp \
         WHERE e.child_cp = ?1 AND e.type IN {IDENTITY_TYPES} AND p.is_orthodox = 1 \
         GROUP BY p.cp, p.char ORDER BY not_simp, not_prc, p.cp"
    ))?;
    let parents: Vec<(i64, String)> = ps
        .query_map([cp], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;

    let (anchor_cp, anchor_char): (i64, String) = match parents.first() {
        Some((pcp, pch)) => (*pcp, pch.clone()),
        None => (cp, ch.to_string()),
    };

    // children of the whole family (every parent's children when this char is derived, else this
    // char's own children), merged per glyph (一字 can be BOTH a PRC-simp and a JP-shinjitai of the
    // same orthodox char, e.g. 学←學: show one 学 branch carrying both reform labels)
    let family: Vec<String> = if parents.is_empty() {
        vec![cp.to_string()]
    } else {
        parents.iter().map(|(pcp, _)| pcp.to_string()).collect()
    };
    // the viewed char's own edge is exempt from the unihan-variant dedup: on 𧿛's page the band
    // must include 𧿛 itself (highlighted as current), even though 蹤's mainstream child is 踪
    let mut s = conn.prepare(&format!(
        "SELECT c.char, c.is_orthodox, e.type, e.reform_id FROM glyph_edge e \
         JOIN character c ON c.cp = e.child_cp \
         WHERE e.parent_cp IN ({parent_list}) AND e.type IN {IDENTITY_TYPES} \
         AND NOT (e.type='simplification' AND e.reform_id='unihan-variant' AND e.child_cp != ?1 \
                  AND EXISTS(SELECT 1 FROM glyph_edge o WHERE o.parent_cp=e.parent_cp \
                             AND o.type='simplification' AND o.reform_id='opencc'))",
        parent_list = family.join(",")
    ))?;
    let rows: Vec<(String, bool, String, Option<String>)> = s
        .query_map([cp], |r| {
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

    let trad_forms: Vec<String> = if parents.is_empty() {
        vec![anchor_char.clone()]
    } else {
        parents.iter().map(|(_, pch)| pch.clone()).collect()
    };
    let mut branches: Vec<FormBranch> = trad_forms
        .iter()
        .map(|form| FormBranch {
            form: form.clone(),
            script: "traditional".into(),
            reform_id: None,
            reform_label: None,
            is_orthodox: true,
        })
        .collect();
    for form in order {
        if trad_forms.contains(&form) {
            continue; // a parent that is also recorded as another parent's child (graph noise)
        }
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
    // separates pure kokuji (峠 辻 凪 榊) from kokuji reborrowed into Chinese (働 腺 畑): Unihan's
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
    let accent = crate::model::ja_reading_accent(conn, id, &variety, reading.as_deref());
    let jyut = crate::model::zh_jyutping(conn, id, &variety);
    Ok(Some(LinkLite { lexeme_id: id, variety, headword, reading, accent, jyut, glosses, relation: relation.to_string(), concept }))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TranslateParams {
    /// an English term (exact concept-label match, case-insensitive)
    pub q: String,
}

/// English-pivot translation: term → concepts → equivalents across all four systems.
#[utoipa::path(
    get, path = "/translate", tag = "dictionary",
    params(TranslateParams),
    responses(
        (status = 200, description = "Concept groups with their members per language", body = TranslateResponse),
        (status = 500, description = "Internal error", body = ApiError),
    )
)]
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
            // English lookup should be precise: only exact-gloss / OMW members (confidence >= 0.7),
            // not the looser content-word token links (0.5) that add cross-topic noise.
            "SELECT DISTINCT s.lexeme_id FROM sense_concept sc \
             JOIN sense s ON s.id = sc.sense_id WHERE sc.concept_id = ?1 AND sc.confidence >= 0.7 LIMIT 40",
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

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SegmentParams {
    /// text containing Han characters to segment
    pub q: String,
}

/// Greedy longest-match segmentation of a Han string into known sub-words, each with a short
/// gloss (紅出口 → 紅 "red" · 出口 "exit"). Non-Han characters are skipped.
#[utoipa::path(
    get, path = "/segment", tag = "dictionary",
    params(SegmentParams),
    responses(
        (status = 200, description = "The segments in order", body = SegmentResponse),
        (status = 500, description = "Internal error", body = ApiError),
    )
)]
pub async fn segment_handler(
    State(st): State<AppState>,
    Query(p): Query<SegmentParams>,
) -> Result<Json<SegmentResponse>, (StatusCode, Json<Value>)> {
    let conn = st.pool.get().map_err(internal)?;
    let resp = build_segment(&conn, &p.q).map_err(internal)?;
    Ok(Json(resp))
}

/// Longest plausible sub-word probed during segmentation (no real dictionary word is longer in
/// practice, and it bounds the number of lookups per position).
const SEG_MAXLEN: usize = 6;

/// Greedy longest-match segmentation of an unrecognized Han query into known sub-words. Each maximal
/// run of Han characters is split left-to-right, always taking the longest substring that is a real
/// word (a `surface_form` lookup) down to length 2, then falling back to a single character. The
/// per-segment short glosses compose the "literally" hint (紅出口 → red · exit).
fn build_segment(conn: &rusqlite::Connection, q: &str) -> rusqlite::Result<SegmentResponse> {
    let mut segments = Vec::new();
    let chars: Vec<char> = q.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !search::is_han(chars[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && search::is_han(chars[j]) {
            j += 1;
        }
        segment_run(conn, &chars[i..j], &mut segments)?;
        i = j;
    }
    Ok(SegmentResponse { query: q.to_string(), segments })
}

fn segment_run(
    conn: &rusqlite::Connection,
    run: &[char],
    out: &mut Vec<SegmentPart>,
) -> rusqlite::Result<()> {
    let mut i = 0;
    while i < run.len() {
        let mut matched = false;
        // try the longest sub-word first, down to a 2-char word
        let maxl = (run.len() - i).min(SEG_MAXLEN);
        let mut len = maxl;
        while len >= 2 {
            let sub: String = run[i..i + len].iter().collect();
            if let Some((id, gloss)) = best_word_gloss(conn, &sub)? {
                out.push(SegmentPart { form: sub, gloss, lexeme_id: Some(id) });
                i += len;
                matched = true;
                break;
            }
            len -= 1;
        }
        if !matched {
            // single character: use the character-table gloss (unchanged from today's breakdown so
            // 紅 stays "red"); fall back to a single-char word gloss only if the character has none.
            let ch = run[i];
            let form = ch.to_string();
            if let Some(gloss) = char_short_gloss(conn, ch)? {
                out.push(SegmentPart { form, gloss, lexeme_id: None });
            } else if let Some((id, gloss)) = best_word_gloss(conn, &form)? {
                out.push(SegmentPart { form, gloss, lexeme_id: Some(id) });
            } else {
                out.push(SegmentPart { form, gloss: String::new(), lexeme_id: None });
            }
            i += 1;
        }
    }
    Ok(())
}

/// The best (most frequent) word whose surface form is exactly `form`, with its cleaned sense-0 gloss.
fn best_word_gloss(
    conn: &rusqlite::Connection,
    form: &str,
) -> rusqlite::Result<Option<(i64, String)>> {
    use rusqlite::OptionalExtension;
    // Prefer the Chinese reading for the literal breakdown (it's a character-by-character Chinese
    // gloss chain), then Cantonese, then Japanese; within a variety, most frequent first. Without the
    // variety order a common Japanese homograph would supply the gloss (火車 → ja "burning cart" rather
    // than zh "train").
    let id: Option<i64> = conn
        .query_row(
            "SELECT l.id FROM surface_form sf JOIN lexeme l ON l.id = sf.lexeme_id \
             WHERE sf.form = ?1 \
             ORDER BY CASE l.variety WHEN 'zh' THEN 0 WHEN 'yue' THEN 1 ELSE 2 END, l.freq DESC, l.id ASC LIMIT 1",
            [form],
            |r| r.get(0),
        )
        .optional()?;
    let Some(id) = id else { return Ok(None) };
    // try several senses: some words' sense 0 is wholly parenthetical / function-word (大廈, 下挫) and
    // cleans to empty: fall through to the next sense so the word still segments as a known word.
    let mut s = conn.prepare("SELECT gloss_en FROM sense WHERE lexeme_id=?1 ORDER BY sense_order LIMIT 8")?;
    let g = s
        .query_map([id], |r| r.get::<_, Option<String>>(0))?
        .filter_map(|r| r.ok().flatten())
        .map(|gl| short_gloss(&gl))
        .find(|x| !x.is_empty())
        .unwrap_or_default();
    if g.is_empty() {
        Ok(None)
    } else {
        Ok(Some((id, g)))
    }
}

/// Character-table gloss for a single character, shortened (the fallback used today for the breakdown).
fn char_short_gloss(conn: &rusqlite::Connection, ch: char) -> rusqlite::Result<Option<String>> {
    use rusqlite::OptionalExtension;
    // the row may EXIST with a NULL gloss_en (≈80k characters), so bind the column as Option<String>
    //: `.optional()` only catches missing rows, not a NULL in a present row (that would 500).
    let g: Option<Option<String>> = conn
        .query_row("SELECT gloss_en FROM character WHERE cp=?1", [ch as i64], |r| {
            r.get::<_, Option<String>>(0)
        })
        .optional()?;
    Ok(g.flatten().map(|g| short_gloss(&g)).filter(|s| !s.is_empty()))
}

/// First NON-EMPTY cleaned sense segment of a gloss (split on ; | , then stripped/lowercased like
/// search). Skips wholly-parenthetical/function-word leading segments instead of returning empty.
fn short_gloss(gloss: &str) -> String {
    for seg in gloss.split([';', '|']) {
        // clean the WHOLE segment first (so a balanced parenthetical "(of sales…) to fall" strips to
        // "to fall"), THEN take the first comma clause for brevity.
        let cleaned = search::clean_segment(seg);
        if !cleaned.is_empty() {
            return cleaned.split(',').next().unwrap_or(&cleaned).trim().to_string();
        }
    }
    String::new()
}

fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, Json<Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
}
