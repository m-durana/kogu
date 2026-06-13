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
    Json(json!({ "status": "ok", "service": "kanzi", "version": env!("CARGO_PKG_VERSION") }))
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
         AND kind NOT IN ('pinyin_num','pinyin_plain')",
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

    // 同字 — other lexemes sharing the backbone key, each labelled cognate / false-friend
    let mut same_form = Vec::new();
    let mut same_form_ids = std::collections::HashSet::new();
    for &other in st.graph.lexemes_by_key(&primary) {
        if other == id {
            continue;
        }
        same_form_ids.insert(other);
        let relation = if shares_concept(conn, id, other)? { "cognate" } else { "false-friend" };
        if let Some(l) = link_lite(conn, other, relation, None)? {
            same_form.push(l);
        }
        if same_form.len() >= 25 {
            break;
        }
    }

    // 同義 — lexemes sharing a concept (different word, same meaning), excluding same-form ones
    let mut translations = Vec::new();
    let mut seen = same_form_ids;
    seen.insert(id);
    let mut s = conn.prepare(
        "SELECT DISTINCT s2.lexeme_id, co.label_en \
         FROM sense_concept sc1 \
         JOIN sense_concept sc2 ON sc2.concept_id = sc1.concept_id \
         JOIN sense s1 ON s1.id = sc1.sense_id \
         JOIN sense s2 ON s2.id = sc2.sense_id \
         JOIN concept co ON co.id = sc1.concept_id \
         WHERE s1.lexeme_id = ?1 AND s2.lexeme_id <> ?1 \
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
    }))
}

/// Do two lexemes share any concept? (cognate vs false-friend discriminator)
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

    Ok(Some(CharInfo {
        ch: ch.to_string(),
        is_orthodox,
        strokes,
        radical,
        ids,
        gloss_en,
        readings,
        variants,
    }))
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
