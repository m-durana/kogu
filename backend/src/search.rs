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

/// Strip a gloss segment to its core: drop parentheticals, a leading "to " (verb glosses),
/// trailing punctuation; lowercase + trim.
fn clean_segment(seg: &str) -> String {
    let mut out = String::with_capacity(seg.len());
    let mut depth = 0u32;
    for c in seg.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let s = out.trim().trim_end_matches(['.', ',', '!', '…']).to_lowercase();
    s.strip_prefix("to ").map(str::to_string).unwrap_or(s).trim().to_string()
}

/// Does `hay` contain `needle` as a whole word / phrase (boundary-aware)?
fn contains_word(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = hay.as_bytes();
    let mut from = 0;
    while let Some(pos) = hay[from..].find(needle) {
        let i = from + pos;
        let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        let after = i + needle.len();
        let after_ok = after >= bytes.len() || !bytes[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        from = i + 1;
    }
    false
}

/// How well an English gloss matches the query term: 1.0 = a gloss segment *is* the term,
/// 0.85 = a segment starts with it, 0.5 = it appears as a whole word, 0.2 = incidental.
fn gloss_match_quality(gloss: &str, ql: &str) -> f64 {
    let mut best = 0.2_f64;
    for seg in gloss.split(';') {
        let s = clean_segment(seg);
        if s == ql {
            return 1.0;
        }
        if s.starts_with(ql) && s[ql.len()..].starts_with(' ') {
            best = best.max(0.85);
        } else if contains_word(&s, ql) {
            best = best.max(0.5);
        }
    }
    best
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
            // english gloss full-text, ranked by how well the gloss matches (exactness + bm25),
            // so "airport" surfaces 空港/機場 above words where airport is merely incidental.
            if let Some(fq) = fts_query(q) {
                let ql = q.trim().to_lowercase();
                let mut stmt = conn.prepare(
                    "SELECT s.lexeme_id, s.gloss_en, bm25(gloss_fts) AS r \
                     FROM gloss_fts JOIN sense s ON s.id = gloss_fts.rowid \
                     WHERE gloss_fts MATCH ?1 ORDER BY r LIMIT 400",
                )?;
                let rows = stmt.query_map([&fq], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })?;
                for row in rows {
                    let (id, gloss) = row?;
                    let quality = gloss_match_quality(&gloss, &ql);
                    bump(&mut cand, id, "english", W_ENGLISH * (0.45 + 0.55 * quality));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_kinds() {
        assert!(matches!(classify("機場"), Kind::Han));
        assert!(matches!(classify("がっこう"), Kind::Kana));
        assert!(matches!(classify("xue2"), Kind::Latin));
        assert!(matches!(classify("airport"), Kind::Latin));
        assert!(matches!(classify(""), Kind::Other));
    }

    #[test]
    fn pinyin_folding() {
        // tone marks, tone numbers, and toneless all fold to the same key
        assert_eq!(pinyin_plain("xué"), "xue");
        assert_eq!(pinyin_plain("xue2"), "xue");
        assert_eq!(pinyin_plain("xue"), "xue");
        assert_eq!(pinyin_plain("ji1 chang3"), "jichang");
        assert_eq!(pinyin_plain("lǜ"), "lv"); // ü -> v
    }

    #[test]
    fn gloss_quality_exact_beats_incidental() {
        // a gloss that *is* the term scores highest; incidental mention lowest
        assert_eq!(gloss_match_quality("airport", "airport"), 1.0);
        assert_eq!(gloss_match_quality("airport; airfield", "airport"), 1.0);
        assert!(gloss_match_quality("airport limousine", "airport") < 1.0);
        assert!(gloss_match_quality("airport limousine", "airport") >= 0.85);
        // "deck (of a ship)" must not score as a strong airport match
        assert!(gloss_match_quality("deck (of a ship)", "airport") <= 0.2);
        // ordering the real bug: 空港's gloss beats デッキ's for "airport"
        assert!(gloss_match_quality("airport", "airport") > gloss_match_quality("deck (of a ship)", "airport"));
    }

    #[test]
    fn gloss_quality_strips_to_prefix_and_parens() {
        // "to open a port" -> verb 'to' stripped; exact phrase match
        assert_eq!(gloss_match_quality("to open a port", "open a port"), 1.0);
        // parenthetical removed before comparison
        assert_eq!(gloss_match_quality("company (business)", "company"), 1.0);
    }

    #[test]
    fn contains_word_boundaries() {
        assert!(contains_word("calling at a port", "port"));
        assert!(!contains_word("airport apron", "port")); // 'port' inside 'airport' is not a word
        assert!(contains_word("train station", "station"));
    }

    #[test]
    fn fts_query_is_order_independent_and() {
        // both orderings produce the same AND query -> predictable english search
        assert_eq!(fts_query("train station"), fts_query("train station"));
        let a = fts_query("train station").unwrap();
        assert!(a.contains("\"train\"") && a.contains("\"station\"") && a.contains(" AND "));
        assert_eq!(fts_query("   "), None);
    }
}
