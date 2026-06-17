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
/// Fold hiragana → katakana (U+3041..3096 +0x60). Native words store kana as hiragana, loanwords as
/// katakana, so folding both query and stored side lets てれび find the katakana-stored テレビ.
fn to_katakana(s: &str) -> String {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if (0x3041..=0x3096).contains(&u) {
                char::from_u32(u + 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}
/// Fold katakana → hiragana (U+30A1..30F6 -0x60).
fn to_hiragana(s: &str) -> String {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if (0x30A1..=0x30F6).contains(&u) {
                char::from_u32(u - 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
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

/// Fold a romaji query to the same canonical key the pipeline stores in romaji_plain: lowercase
/// a–z, macrons unfolded, n before a labial, long vowels collapsed (tokyo=toukyou=tōkyō,
/// shinbun=shimbun). Mirrors pipeline/kogupipe/ingest/romaji.py::fold.
pub fn romaji_plain(q: &str) -> String {
    let mut s = String::with_capacity(q.len());
    for ch in q.chars() {
        let c = match ch {
            'ā' | 'â' => 'a',
            'ī' | 'î' => 'i',
            'ū' | 'û' => 'u',
            'ē' | 'ê' => 'e',
            'ō' | 'ô' => 'o',
            c => c,
        };
        for low in c.to_lowercase() {
            if low.is_ascii_alphabetic() {
                s.push(low);
            }
        }
    }
    for lab in ["mb", "mp", "mm"] {
        s = s.replace(lab, &format!("n{}", &lab[1..]));
    }
    for (a, b) in [("ou", "o"), ("oo", "o"), ("uu", "u"), ("ee", "e"), ("ei", "e"), ("aa", "a"), ("ii", "i")] {
        s = s.replace(a, b);
    }
    s
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

/// Light English stemmer for RANKING alignment only (FTS porter handles retrieval): strip a common
/// inflectional suffix so "ears"→"ear", "studies"→"study", "loved"→"lov", "running"→"runn". It does
/// NOT need to be linguistically perfect — `stem_close` absorbs porter's silent-e (lov ≈ love).
fn stem_word(w: &str) -> String {
    for (suf, rep) in [
        ("ies", "y"),
        ("ied", "y"),
        ("ying", ""),
        ("ing", ""),
        ("edly", ""),
        ("ed", ""),
        ("es", ""),
        ("s", ""),
    ] {
        if let Some(stem) = w.strip_suffix(suf) {
            if stem.len() >= 3 {
                return format!("{stem}{rep}");
            }
        }
    }
    w.to_string()
}
fn stem_phrase(s: &str) -> String {
    s.split(' ').map(stem_word).collect::<Vec<_>>().join(" ")
}
/// Equal modulo a trailing silent-e the crude stemmer can't recover (loved→lov ≈ love→love).
fn stem_close(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.len() >= 3 && long.starts_with(short) && long.len() - short.len() <= 1
}

/// How well an English gloss matches the query term: 1.0 = a gloss segment *is* the term,
/// 0.85 = a segment starts with it, 0.5 = it appears as a whole word, 0.2 = incidental. Segments
/// split on both ';' and '|' (CC-CEDICT pipes); compared on stems so inflected queries align.
fn gloss_match_quality(gloss: &str, ql: &str) -> f64 {
    let qd = stem_phrase(ql);
    let mut best = 0.2_f64;
    for seg in gloss.split([';', '|']) {
        let s = clean_segment(seg);
        let sd = stem_phrase(&s);
        if s == ql || sd == qd || stem_close(&sd, &qd) {
            return 1.0;
        }
        if sd.starts_with(&qd) && sd[qd.len()..].starts_with(' ') {
            best = best.max(0.85);
        } else if contains_word(&sd, &qd) {
            best = best.max(0.5);
        }
    }
    best
}

/// Map gloss-match quality to a retrieval weight with a DECISIVE exact-sense bonus: when the query
/// term IS a full sense of an entry, that must beat a fringe entry that merely mentions it — even one
/// with far higher frequency. So "ear" → 耳 (a sense "ear"), not 稲穂 ("ear of rice"). The old linear
/// 0.45+0.55·quality let frequency swamp the meaning signal; these tiers don't.
fn english_weight(quality: f64) -> f64 {
    if quality >= 1.0 {
        0.9 // a sense exactly equals the query
    } else if quality >= 0.85 {
        0.6 // a sense starts with the query
    } else if quality >= 0.5 {
        0.45 // appears as a whole word
    } else {
        0.3 // incidental co-occurrence
    }
}

/// Count glosses that carry real meaning, ignoring bare cross-references ("used in 洗馬", "variant of
/// X", "see Y", "surname Z", "abbr…"). Used only as a deterministic ranking tiebreak so the richer
/// reading of a homograph leads — mirrors the frontend `isMinorGloss` so backend rank and the def the
/// UI keeps agree. Lowercased prefix check; deliberately cheap.
fn meaningful_gloss_count(glosses: &[String]) -> usize {
    glosses
        .iter()
        .filter(|g| {
            let s = g.trim().to_lowercase();
            !s.is_empty()
                && !s.starts_with("used in")
                && !s.starts_with("variant of")
                && !s.starts_with("old variant of")
                && !s.starts_with("see ")
                && !s.starts_with("surname ")
                && !s.starts_with("abbr")
        })
        .count()
}

const W_EXACT: f64 = 1.0;
const W_VARIANT: f64 = 0.85;
const W_READING: f64 = 0.72;
const W_READING_PREFIX: f64 = 0.55; // as-you-type prefix (たべ→たべる); below an exact reading
// frequency is a gentle ADDITIVE tiebreak, never a multiplier: the old `weight * (1 + freq)` let a
// frequent fringe word (freq→2× boost) outrank an exact match. Additive + small keeps the match tier
// dominant (an exact reading always beats a prefix; an exact sense always beats an incidental one).
const FREQ_BONUS: f64 = 0.15;

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

    // exact written-form match - works for ANY typed string (機場, 甘い, 食べる, あまい, …).
    // (Mixed kanji+kana words classify as Kana but their written form lives in surface_form.)
    {
        let mut stmt = conn.prepare("SELECT lexeme_id FROM surface_form WHERE form = ?1")?;
        let ids: Vec<i64> = stmt.query_map([q], |r| r.get(0))?.collect::<Result<_, _>>()?;
        for id in ids {
            bump(&mut cand, id, "exact", W_EXACT);
        }
    }
    // backbone-key expansion whenever the query contains Han (cross-script / 同字)
    if q.chars().any(is_han) {
        for &id in state.graph.lexemes_by_key(q) {
            bump(&mut cand, id, "variant", W_VARIANT);
        }
    }

    match kind {
        Kind::Kana => {
            // fold the query to BOTH kana scripts so a hiragana query matches katakana-stored
            // loanwords (てれび→テレビ) and vice-versa, regardless of how the reading was stored.
            let hira = to_hiragana(q);
            let kata = to_katakana(q);
            {
                let mut stmt = conn.prepare(
                    "SELECT lexeme_id FROM lexeme_reading WHERE kind='kana' AND value IN (?1, ?2)",
                )?;
                let ids: Vec<i64> =
                    stmt.query_map([&hira, &kata], |r| r.get(0))?.collect::<Result<_, _>>()?;
                for id in ids {
                    bump(&mut cand, id, "reading", W_READING);
                }
            }
            // prefix (as-you-type): typing たべ should already surface たべる, がっこ→がっこう. Needs
            // ≥2 kana so a single mora doesn't match half the dictionary. Scored below an exact reading.
            if hira.chars().count() >= 2 {
                let hp = format!("{hira}%");
                let kp = format!("{kata}%");
                let mut stmt = conn.prepare(
                    "SELECT lexeme_id FROM lexeme_reading \
                     WHERE kind='kana' AND (value LIKE ?1 OR value LIKE ?2) LIMIT 200",
                )?;
                let ids: Vec<i64> =
                    stmt.query_map([&hp, &kp], |r| r.get(0))?.collect::<Result<_, _>>()?;
                for id in ids {
                    bump(&mut cand, id, "reading", W_READING_PREFIX);
                }
            }
            // surface prefix for mixed kanji+kana words: 食べ → 食べる, 高 not reached here (Han-only).
            if q.chars().count() >= 2 {
                let sp = format!("{q}%");
                let mut stmt =
                    conn.prepare("SELECT lexeme_id FROM surface_form WHERE form LIKE ?1 LIMIT 200")?;
                let ids: Vec<i64> =
                    stmt.query_map([&sp], |r| r.get(0))?.collect::<Result<_, _>>()?;
                for id in ids {
                    bump(&mut cand, id, "reading", W_READING_PREFIX);
                }
            }
        }
        Kind::Latin => {
            // phonetic (toneless fold) - tolerant of tone marks / numbers / no tone.
            // pinyin_plain and jyutping_plain share the same fold (letters only), so one key
            // matches Mandarin *and* Cantonese readings (jyutping was the original's blind spot).
            let plain = pinyin_plain(q);
            if !plain.is_empty() {
                let mut stmt = conn.prepare(
                    "SELECT lexeme_id FROM lexeme_reading \
                     WHERE kind IN ('pinyin_plain','jyutping_plain') AND value = ?1",
                )?;
                let ids: Vec<i64> =
                    stmt.query_map([&plain], |r| r.get(0))?.collect::<Result<_, _>>()?;
                for id in ids {
                    bump(&mut cand, id, "reading", W_READING);
                }
            }
            // romaji reading (Japanese): tolerant of long-vowel / n-m spelling (tokyo, toukyou, …)
            let rp = romaji_plain(q);
            if rp.len() >= 2 {
                let mut stmt = conn.prepare(
                    "SELECT lexeme_id FROM lexeme_reading WHERE kind='romaji_plain' AND value = ?1",
                )?;
                let ids: Vec<i64> =
                    stmt.query_map([&rp], |r| r.get(0))?.collect::<Result<_, _>>()?;
                for id in ids {
                    bump(&mut cand, id, "reading", W_READING);
                }
            }
            // english gloss full-text, ranked by how well the gloss matches (exactness + bm25),
            // so "airport" surfaces 空港/機場 above words where airport is merely incidental.
            if let Some(fq) = fts_query(q) {
                let ql = q.trim().to_lowercase();
                let mut stmt = conn.prepare(
                    "SELECT s.lexeme_id, s.gloss_en, s.sense_order, bm25(gloss_fts) AS r \
                     FROM gloss_fts JOIN sense s ON s.id = gloss_fts.rowid \
                     WHERE gloss_fts MATCH ?1 ORDER BY r LIMIT 400",
                )?;
                let rows = stmt.query_map([&fq], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
                })?;
                for row in rows {
                    let (id, gloss, sense_order) = row?;
                    let quality = gloss_match_quality(&gloss, &ql);
                    let mut w = english_weight(quality);
                    // the query IS this word's PRIMARY meaning (exact match on sense 0) — a stronger
                    // signal than an exact match on a word's minor sense, and big enough to beat the
                    // (unreliable) frequency tiebreak: 山 leads "mountain", not 深山 ("deep mountains"),
                    // whose bare "mountain" sense is secondary.
                    if quality >= 1.0 && sense_order == 0 {
                        w += FREQ_BONUS;
                    }
                    bump(&mut cand, id, "english", w);
                }
            }
        }
        _ => {}
    }

    // assemble + rank
    let mut hits: Vec<Hit> = Vec::with_capacity(cand.len());
    for (id, (mt, w)) in cand {
        if let Some(hit) = build_hit(conn, id, mt, w, pref_script)? {
            hits.push(hit);
        }
    }
    // Rank by score, then DETERMINISTIC tiebreaks — without them, equal-score homographs (洗 xǐ "to
    // wash" vs xiǎn "used in 洗馬", same freq → same score) were ordered by HashMap iteration, which is
    // randomly seeded per request, so the same character led with a different reading on each visit.
    // Tiebreak on richer-meaning first (more non-cross-reference glosses), then lowest lexeme_id, so a
    // glyph always resolves to the same primary reading.
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(meaningful_gloss_count(&b.glosses).cmp(&meaningful_gloss_count(&a.glosses)))
            .then(a.lexeme_id.cmp(&b.lexeme_id))
    });

    // kokuji fallback: a single valid character with no word-lexeme (峠 has one; 込/凪 don't)
    // still deserves a character page. Synthesise a hit keyed by a negative codepoint id.
    if kind == Kind::Han && hits.is_empty() {
        let mut chars = q.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            if let Some(hit) = char_only_hit(conn, ch)? {
                hits.push(hit);
            }
        }
    }
    hits.truncate(limit);

    Ok(SearchResponse { query: q.to_string(), classified_as: kind.as_str().to_string(), results: hits })
}

/// Guess a character's variety for display: kana on/kun present → Japanese, else Chinese.
pub fn char_variety(conn: &Connection, cp: i64) -> rusqlite::Result<&'static str> {
    let has_kana: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM char_reading WHERE cp=?1 AND kind IN ('onyomi','kunyomi'))",
        [cp],
        |r| r.get(0),
    )?;
    let has_pinyin: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM char_reading WHERE cp=?1 AND kind='pinyin')",
        [cp],
        |r| r.get(0),
    )?;
    Ok(if has_pinyin == 0 && has_kana != 0 { "ja" } else { "zh" })
}

/// A character that exists but has no word-lexeme → a synthetic hit (negative codepoint id) so the
/// frontend can still open a character page. Returns None if the character isn't in the DB.
fn char_only_hit(conn: &Connection, ch: char) -> rusqlite::Result<Option<Hit>> {
    let cp = ch as i64;
    let gloss: Option<String> = conn
        .query_row("SELECT gloss_en FROM character WHERE cp=?1", [cp], |r| r.get(0))
        .ok()
        .flatten();
    // require the character to actually exist (a reading or a gloss)
    let exists: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM character WHERE cp=?1)",
        [cp],
        |r| r.get(0),
    )?;
    if exists == 0 {
        return Ok(None);
    }
    let variety = char_variety(conn, cp)?;
    let kind = if variety == "ja" { "kana" } else { "pinyin" };
    let reading: Option<String> = conn
        .query_row(
            "SELECT value FROM char_reading WHERE cp=?1 AND kind=?2 LIMIT 1",
            rusqlite::params![cp, kind],
            |r| r.get(0),
        )
        .ok();
    Ok(Some(Hit {
        lexeme_id: -cp,
        variety: variety.to_string(),
        headword: ch.to_string(),
        reading,
        forms: vec![Form { form: ch.to_string(), script: "other".into(), region: None, is_primary: true }],
        glosses: gloss.into_iter().collect(),
        match_type: "exact".into(),
        score: 1.0,
    }))
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

    // freq factor: ranked words in [0,1]; unranked get a low baseline so any frequency signal beats
    // none (a common word outranks an unknown one), but only as a tiebreak within a match tier.
    let freq_factor = freq.unwrap_or(0.1);
    let mut score = weight + FREQ_BONUS * freq_factor;
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
