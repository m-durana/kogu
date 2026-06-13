//! Query pipeline (DESIGN.md §3): classify -> expand/normalise -> retrieve -> rank.
//! Never collapses to one canonical form, never bails on ambiguity: every candidate is kept
//! with a score; ranking is by match-type × frequency, never by string length.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::model::{Form, Hit, SearchResponse};
use crate::state::AppState;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Han,
    Kana,
    Latin,
    Other,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Han => "han",
            Kind::Kana => "kana",
            Kind::Latin => "latin",
            Kind::Other => "other",
        }
    }
}

fn is_han(ch: char) -> bool {
    let c = ch as u32;
    (0x3400..=0x9FFF).contains(&c) || (0x20000..=0x3FFFF).contains(&c) || (0xF900..=0xFAFF).contains(&c)
}
fn is_kana(ch: char) -> bool {
    let c = ch as u32;
    (0x3040..=0x30FF).contains(&c) || (0xFF66..=0xFF9D).contains(&c)
}

pub fn classify(q: &str) -> Kind {
    let q = q.trim();
    if q.is_empty() {
        return Kind::Other;
    }
    if q.chars().any(is_kana) {
        return Kind::Kana;
    }
    if q.chars().any(is_han) {
        return Kind::Han;
    }
    if q.chars().any(|c| c.is_ascii_alphabetic()) {
        return Kind::Latin;
    }
    Kind::Other
}

/// Tone-mark / tone-number / toneless pinyin all fold to the same toneless key.
pub fn pinyin_plain(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for ch in q.chars() {
        let base = match ch {
            'ā' | 'á' | 'ǎ' | 'à' | 'a' => 'a',
            'ē' | 'é' | 'ě' | 'è' | 'e' => 'e',
            'ī' | 'í' | 'ǐ' | 'ì' | 'i' => 'i',
            'ō' | 'ó' | 'ǒ' | 'ò' | 'o' => 'o',
            'ū' | 'ú' | 'ǔ' | 'ù' | 'u' => 'u',
            'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' | 'ü' => 'v',
            c if c.is_ascii_alphabetic() => c.to_ascii_lowercase(),
            _ => continue, // drop spaces, digits, apostrophes
        };
        out.push(base);
    }
    out
}

/// Build a safe FTS5 AND-query so word order doesn't change results (predictable English search).
fn fts_query(q: &str) -> Option<String> {
    let tokens: Vec<String> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.to_lowercase()))
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" AND "))
    }
}

const W_EXACT: f64 = 1.0;
const W_VARIANT: f64 = 0.85;
const W_READING: f64 = 0.72;
const W_ENGLISH: f64 = 0.5;

pub fn search(
    state: &AppState,
    conn: &Connection,
    q: &str,
    pref_script: Option<&str>,
    limit: usize,
) -> rusqlite::Result<SearchResponse> {
    let q = q.trim();
    let kind = classify(q);
    // best base weight per lexeme + how it matched
    let mut cand: HashMap<i64, (&'static str, f64)> = HashMap::new();
    let bump = |cand: &mut HashMap<i64, (&'static str, f64)>, id: i64, mt: &'static str, w: f64| {
        let e = cand.entry(id).or_insert((mt, 0.0));
        if w > e.1 {
            *e = (mt, w);
        }
    };

    match kind {
        Kind::Han => {
            // exact surface-form match
            let mut stmt = conn.prepare("SELECT lexeme_id FROM surface_form WHERE form = ?1")?;
            let ids: Vec<i64> = stmt.query_map([q], |r| r.get(0))?.collect::<Result<_, _>>()?;
            for id in ids {
                bump(&mut cand, id, "exact", W_EXACT);
            }
            // variant / cross-script via backbone key (also yields 同字 cross-language hits)
            for &id in state.graph.lexemes_by_key(q) {
                bump(&mut cand, id, "variant", W_VARIANT);
            }
        }
        Kind::Kana => {
            let mut stmt = conn
                .prepare("SELECT lexeme_id FROM lexeme_reading WHERE kind='kana' AND value = ?1")?;
            let ids: Vec<i64> = stmt.query_map([q], |r| r.get(0))?.collect::<Result<_, _>>()?;
            for id in ids {
                bump(&mut cand, id, "reading", W_READING);
            }
        }
        Kind::Latin => {
            // pinyin (toneless fold) — tolerant of tone marks / numbers / no tone
            let plain = pinyin_plain(q);
            if !plain.is_empty() {
                let mut stmt = conn.prepare(
                    "SELECT lexeme_id FROM lexeme_reading WHERE kind='pinyin_plain' AND value = ?1",
                )?;
                let ids: Vec<i64> =
                    stmt.query_map([&plain], |r| r.get(0))?.collect::<Result<_, _>>()?;
                for id in ids {
                    bump(&mut cand, id, "reading", W_READING);
                }
            }
            // english gloss full-text
            if let Some(fq) = fts_query(q) {
                let mut stmt = conn.prepare(
                    "SELECT DISTINCT s.lexeme_id FROM gloss_fts \
                     JOIN sense s ON s.id = gloss_fts.rowid \
                     WHERE gloss_fts MATCH ?1 LIMIT 400",
                )?;
                let ids: Vec<i64> =
                    stmt.query_map([&fq], |r| r.get(0))?.collect::<Result<_, _>>()?;
                for id in ids {
                    bump(&mut cand, id, "english", W_ENGLISH);
                }
            }
        }
        Kind::Other => {}
    }

    // assemble + rank
    let mut hits: Vec<Hit> = Vec::with_capacity(cand.len());
    for (id, (mt, w)) in cand {
        if let Some(hit) = build_hit(conn, id, mt, w, pref_script)? {
            hits.push(hit);
        }
    }
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);

    Ok(SearchResponse { query: q.to_string(), classified_as: kind.as_str().to_string(), results: hits })
}

fn build_hit(
    conn: &Connection,
    id: i64,
    match_type: &str,
    weight: f64,
    pref_script: Option<&str>,
) -> rusqlite::Result<Option<Hit>> {
    let row = conn.query_row(
        "SELECT variety, headword, reading, freq FROM lexeme WHERE id = ?1",
        [id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<f64>>(3)?)),
    );
    let (variety, headword, reading, freq) = match row {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e),
    };

    let mut fstmt = conn.prepare(
        "SELECT form, script, region, is_primary FROM surface_form WHERE lexeme_id = ?1",
    )?;
    let forms: Vec<Form> = fstmt
        .query_map([id], |r| {
            Ok(Form {
                form: r.get(0)?,
                script: r.get(1)?,
                region: r.get(2)?,
                is_primary: r.get::<_, i64>(3)? != 0,
            })
        })?
        .collect::<Result<_, _>>()?;

    let mut gstmt =
        conn.prepare("SELECT gloss_en FROM sense WHERE lexeme_id = ?1 ORDER BY sense_order")?;
    let glosses: Vec<String> =
        gstmt.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;

    // freq factor: known freq in [0,1]; unknown (most zh) gets a neutral baseline
    let freq_factor = freq.unwrap_or(0.4);
    let mut score = weight * (1.0 + freq_factor);
    // gentle script preference (not length-based ranking)
    if let Some(ps) = pref_script {
        if forms.iter().any(|f| f.script == ps) {
            score *= 1.05;
        }
    }

    Ok(Some(Hit {
        lexeme_id: id,
        variety,
        headword,
        reading,
        forms,
        glosses,
        match_type: match_type.to_string(),
        score,
    }))
}
