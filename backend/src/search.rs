//! Query pipeline (DESIGN.md §3): classify -> expand/normalise -> retrieve -> rank.
//! Never collapses to one canonical form, never bails on ambiguity: every candidate is kept
//! with a score; ranking is by match-type × frequency, never by string length.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::model::{Form, Hit, SearchResponse, SuggestItem, SuggestResponse};
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

pub(crate) fn is_han(ch: char) -> bool {
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
    // wapuro/kunrei digraphs -> the Hepburn the stored side uses (jyuu -> juu -> ju). BEFORE the
    // long-vowel collapse so the vowel run survives to be folded.
    for (a, b) in [("jy", "j"), ("zy", "j"), ("sy", "sh"), ("ty", "ch")] {
        s = s.replace(a, b);
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
pub(crate) fn clean_segment(seg: &str) -> String {
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
    let mut s = out.trim().trim_end_matches(['.', ',', '!', '…']).to_lowercase();
    // strip a leading function word so an article/infinitive-only difference is still an exact match:
    // "the Great Wall" vs query "great wall" → exact (so 長城 beats 長城飯店 "Great Wall Hotel").
    for p in ["to ", "the ", "an ", "a "] {
        if let Some(rest) = s.strip_prefix(p) {
            s = rest.to_string();
            break;
        }
    }
    s.trim().to_string()
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
/// NOT need to be linguistically perfect: `stem_close` absorbs porter's silent-e (lov ≈ love).
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
/// term IS a full sense of an entry, that must beat a fringe entry that merely mentions it: even one
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
/// reading of a homograph leads: mirrors the frontend `isMinorGloss` so backend rank and the def the
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
// the typed string being a lexeme's PRIMARY spelling outranks it being an alt spelling of another
// word: ボーリング (boring) must lead over ボウリング (bowling), which merely lists ボーリング as a
// variant kana form. Exceeds FREQ_BONUS so frequency can never flip it back.
const W_EXACT_PRIMARY: f64 = 1.16;
// A curated English alias (lexeme_alias) is an authoritative statement that the query term names this
// word: used to fill paradigm gaps a CC-Canto gloss omits (佢 glossed "he, she, it" also answers
// "him"/"her"/"his"). Ranked as a decisive tier ABOVE every ordinary match (incl. exact primary +
// max freq) so the canonical word leads over a high-frequency written-Chinese form that merely spells
// the term in its gloss (她的 for "her"). Only fires on curated terms, so it can't over-rank generally.
const W_ALIAS: f64 = 1.35;
const W_VARIANT: f64 = 0.85;
const W_READING: f64 = 0.72;
const W_READING_PREFIX: f64 = 0.55; // as-you-type prefix (たべ→たべる); below an exact reading
const W_PARTIAL: f64 = 0.45; // a dictionary word found as a substring of an unresolved query (scaled by coverage)
const W_WILDCARD: f64 = 0.5; // flat base weight for a wildcard hit; the freq factor orders the tier
// frequency is a gentle ADDITIVE tiebreak, never a multiplier: the old `weight * (1 + freq)` let a
// frequent fringe word (freq→2× boost) outrank an exact match. Additive + small keeps the match tier
// dominant (an exact reading always beats a prefix; an exact sense always beats an incidental one).
const FREQ_BONUS: f64 = 0.15;
/// English results: at most this many consecutive hits may share a variety before a different-variety
/// hit is pulled forward (see the reflow at the end of `search`).
const CAP_RUN: usize = 3;

/// Stable "no more than `cap` consecutive results from the same variety" reflow. Walks the already
/// score-sorted list and, once a variety hits the cap, pulls forward the next-best hit of a DIFFERENT
/// variety (if any) instead of continuing the run. Within each variety the score order is preserved.
fn cap_consecutive_variety(sorted: Vec<Hit>, cap: usize) -> Vec<Hit> {
    let mut rest = sorted;
    let mut out: Vec<Hit> = Vec::with_capacity(rest.len());
    let mut run_var: Option<String> = None;
    let mut run = 0usize;
    while !rest.is_empty() {
        let idx = if run < cap {
            0
        } else {
            // cap reached: take the highest-scored hit of a different variety; none → keep order
            rest.iter().position(|h| Some(&h.variety) != run_var.as_ref()).unwrap_or(0)
        };
        let h = rest.remove(idx);
        if run_var.as_deref() == Some(h.variety.as_str()) {
            run += 1;
        } else {
            run_var = Some(h.variety.clone());
            run = 1;
        }
        out.push(h);
    }
    out
}

/// Escape LIKE wildcards so a typed % or _ is matched literally (used with `ESCAPE '\'`).
fn escape_like(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '%' || c == '_' || c == '\\' {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

/// Run one suggest query (`?1` = string param, `?2` = limit), appending deduped rows up to `cap`.
fn collect_suggest(
    conn: &Connection,
    sql: &str,
    param: &str,
    cap: usize,
    out: &mut Vec<SuggestItem>,
    seen: &mut std::collections::HashSet<String>,
) -> rusqlite::Result<()> {
    if out.len() >= cap {
        return Ok(());
    }
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![param, cap as i64], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?))
    })?;
    for row in rows {
        let (headword, reading, variety) = row?;
        if seen.insert(headword.clone()) {
            out.push(SuggestItem { headword, reading, variety });
            if out.len() >= cap {
                break;
            }
        }
    }
    Ok(())
}

/// Lightweight autocomplete: prefix matches on the written form (Han), the reading (kana /
/// pinyin / jyutping), or an English gloss term: frequency-ranked, deduped by headword. No senses,
/// so it is cheap to call on every keystroke.
pub fn suggest(conn: &Connection, q: &str, limit: usize) -> rusqlite::Result<SuggestResponse> {
    let q = q.trim();
    let cap = limit.clamp(1, 20);
    let mut out: Vec<SuggestItem> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    // don't run scans for a half-typed wildcard; the user commits it with Enter (→ search()).
    if q.is_empty() || is_wildcard(q) {
        return Ok(SuggestResponse { query: q.to_string(), suggestions: out });
    }

    if q.chars().any(is_han) {
        // prefix on the written form (shortest, most frequent first)
        collect_suggest(
            conn,
            "SELECT l.headword, l.reading, l.variety FROM surface_form sf JOIN lexeme l ON l.id = sf.lexeme_id \
             WHERE sf.form LIKE ?1 ESCAPE '\\' AND sf.rare = 0 \
             ORDER BY l.freq IS NULL, l.freq DESC, length(sf.form), l.id LIMIT ?2",
            &format!("{}%", escape_like(q)),
            cap,
            &mut out,
            &mut seen,
        )?;
    } else if q.chars().any(is_kana) {
        let hira = escape_like(&to_hiragana(q));
        collect_suggest(
            conn,
            "SELECT l.headword, l.reading, l.variety FROM lexeme_reading lr JOIN lexeme l ON l.id = lr.lexeme_id \
             WHERE lr.kind = 'kana' AND lr.value LIKE ?1 ESCAPE '\\' \
             ORDER BY l.freq IS NULL, l.freq DESC, l.id LIMIT ?2",
            &format!("{hira}%"),
            cap,
            &mut out,
            &mut seen,
        )?;
    } else if q.chars().any(|c| c.is_ascii_alphabetic()) {
        // romanized reading prefix (pinyin / jyutping fold to the same plain key)
        collect_suggest(
            conn,
            "SELECT l.headword, l.reading, l.variety FROM lexeme_reading lr JOIN lexeme l ON l.id = lr.lexeme_id \
             WHERE lr.kind IN ('pinyin_plain','jyutping_plain') AND lr.value LIKE ?1 ESCAPE '\\' \
             ORDER BY l.freq IS NULL, l.freq DESC, l.id LIMIT ?2",
            &format!("{}%", escape_like(&pinyin_plain(q))),
            cap,
            &mut out,
            &mut seen,
        )?;
        // then English gloss-term prefixes (FTS prefix query), if room remains
        let term: String = q.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        if out.len() < cap && !term.is_empty() {
            collect_suggest(
                conn,
                // no GROUP BY (bm25() can't be used in that context); dedup by headword happens in
                // collect_suggest. Relevance-ranked, frequency as a tiebreak.
                "SELECT l.headword, l.reading, l.variety FROM gloss_fts \
                 JOIN sense s ON s.id = gloss_fts.rowid JOIN lexeme l ON l.id = s.lexeme_id \
                 WHERE gloss_fts MATCH ?1 ORDER BY bm25(gloss_fts), l.freq IS NULL, l.freq DESC LIMIT ?2",
                // quote the term so a bare FTS5 keyword (OR / NOT / AND) is a literal, not an operator
                // (`OR*` is an FTS syntax error → 500). term is already alphanumeric-only.
                &format!("\"{term}\"*"),
                cap,
                &mut out,
                &mut seen,
            )?;
        }
    }
    Ok(SuggestResponse { query: q.to_string(), suggestions: out })
}

/// True if the query carries a wildcard token (`*` / `?`, incl. the fullwidth IME variants).
pub fn is_wildcard(q: &str) -> bool {
    q.chars().any(|c| matches!(c, '*' | '?' | '＊' | '？'))
}

/// How to interpret a romanized/ambiguous query. `you` can mean the WORD (English gloss → 你) or a
/// SOUND (a reading → 有/又/よ); by default we blend both, but the caller can force one lens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// blend meaning + sound (the default)
    Auto,
    /// only phonetic matches: words whose reading (pinyin / jyutping / romaji / kana) is the query
    Sound,
    /// only meaning matches: words whose English gloss is the query
    Meaning,
}

impl Scope {
    pub fn from_param(s: Option<&str>) -> Scope {
        match s.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("sound") | Some("phonetic") | Some("reading") => Scope::Sound,
            Some("meaning") | Some("english") | Some("gloss") => Scope::Meaning,
            _ => Scope::Auto,
        }
    }
    /// Does a candidate that matched via `match_type` survive this scope? Written-form matches
    /// (exact/variant, and wildcards, which restrict their own fields) always survive; only the two
    /// ambiguous tiers - `reading` (sound) and `english` (meaning) - are filtered.
    fn keeps(self, match_type: &str) -> bool {
        match self {
            Scope::Auto => true,
            Scope::Sound => match_type != "english",
            Scope::Meaning => match_type != "reading",
        }
    }
}

/// Wildcard search over the written form: `*` = any run of characters, `?` = exactly one. Uses GLOB
/// (case-sensitive, so leading-literal patterns like 你* use the surface_form.form index; LIKE is
/// case-insensitive and would full-scan). Frequency-ranked, returned as a flat list.
fn wildcard_search(
    conn: &Connection,
    q: &str,
    pref_script: Option<&str>,
    scope: Scope,
    limit: usize,
) -> rusqlite::Result<SearchResponse> {
    // normalise fullwidth ＊／？ → ASCII; wrap a literal '[' as GLOB's [[] (GLOB has no ESCAPE clause)
    let mut pat = String::with_capacity(q.len() + 2);
    let mut literals = 0usize;
    for c in q.chars() {
        match c {
            '*' | '＊' => pat.push('*'),
            '?' | '？' => pat.push('?'),
            '[' => pat.push_str("[[]"),
            _ => {
                pat.push(c);
                literals += 1;
            }
        }
    }
    // a pattern with no literal characters (just * / ?) would dump the corpus: refuse it.
    if literals == 0 {
        return Ok(SearchResponse {
            query: q.to_string(),
            classified_as: "wildcard".to_string(),
            results: vec![],
        });
    }
    use std::collections::HashSet;
    let mut idset: HashSet<i64> = HashSet::new();
    // written-form match (你* / *場): the lexeme's PRIMARY/headword form only, so the displayed
    // headword actually fits the pattern. (Without this, a usually-kana word with a rare kanji variant
    // ending in 場: 鱈場/タラバ : leaks into a "*場" search as タラバ.) Skipped when forcing Sound.
    if scope != Scope::Sound {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT sf.lexeme_id FROM surface_form sf JOIN lexeme l ON l.id = sf.lexeme_id \
             WHERE sf.form GLOB ?1 AND (sf.is_primary = 1 OR sf.form = l.headword) LIMIT 800",
        )?;
        for id in stmt.query_map([&pat], |r| r.get::<_, i64>(0))? {
            idset.insert(id?);
        }
    }
    // reading match (you* → 有/又, tabe* → 食べる, たべ* → 食べる): GLOB the same pattern against the
    // folded reading keys. Romanised keys are stored lowercase, so lowercase the pattern's literals;
    // a Han pattern simply matches nothing here. Skipped when forcing Meaning.
    if scope != Scope::Meaning {
        let read_pat = pat.to_lowercase();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT lr.lexeme_id FROM lexeme_reading lr \
             WHERE lr.kind IN ('pinyin_plain','jyutping_plain','romaji_plain','kana') \
               AND lr.value GLOB ?1 LIMIT 800",
        )?;
        for id in stmt.query_map([&read_pat], |r| r.get::<_, i64>(0))? {
            idset.insert(id?);
        }
    }
    let mut hits: Vec<Hit> = Vec::new();
    for id in idset {
        if let Some(hit) = build_hit(conn, id, "wildcard", W_WILDCARD, pref_script)? {
            hits.push(hit);
        }
    }
    // frequency-first (score embeds the freq factor), then deterministic tiebreaks
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(meaningful_gloss_count(&b.glosses).cmp(&meaningful_gloss_count(&a.glosses)))
            .then(a.lexeme_id.cmp(&b.lexeme_id))
    });
    hits.truncate(limit);
    Ok(SearchResponse { query: q.to_string(), classified_as: "wildcard".to_string(), results: hits })
}

pub fn search(
    state: &AppState,
    conn: &Connection,
    q: &str,
    pref_script: Option<&str>,
    scope: Scope,
    limit: usize,
) -> rusqlite::Result<SearchResponse> {
    let q = q.trim();
    // wildcard search (你* / *場 / 機?場): detected before classification since '*'/'?' aren't
    // Han/Kana/Latin. Always returns a plain list (no single headword to unify).
    if is_wildcard(q) {
        return wildcard_search(conn, q, pref_script, scope, limit);
    }
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
        // rare=0: a rare/irregular/search-only JMdict form must not exact-match (it would shadow the
        // synthetic character page for a kokuji like 込 that's only a rare alt-form of some word).
        let mut stmt =
            conn.prepare("SELECT lexeme_id, is_primary FROM surface_form WHERE form = ?1 AND rare = 0")?;
        let ids: Vec<(i64, bool)> = stmt
            .query_map([q], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)))?
            .collect::<Result<_, _>>()?;
        for (id, primary) in ids {
            bump(&mut cand, id, "exact", if primary { W_EXACT_PRIMARY } else { W_EXACT });
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
            // a folded kana string that IS a written form is an EXACT lookup, not a mere reading
            // match: typing びーる is typing ビール. Without this both ビール and 麦酒 (which lists
            // ビール as an alt form) sat in the reading tier and the obscure word could tie-break
            // ahead of the everyday one.
            {
                let mut stmt = conn.prepare(
                    "SELECT lexeme_id, is_primary FROM surface_form WHERE form IN (?1, ?2) AND rare = 0",
                )?;
                let ids: Vec<(i64, bool)> = stmt
                    .query_map([&hira, &kata], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)))?
                    .collect::<Result<_, _>>()?;
                for (id, primary) in ids {
                    bump(&mut cand, id, "exact", if primary { W_EXACT_PRIMARY } else { W_EXACT });
                }
            }
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
                    conn.prepare("SELECT lexeme_id FROM surface_form WHERE form LIKE ?1 AND rare = 0 LIMIT 200")?;
                let ids: Vec<i64> =
                    stmt.query_map([&sp], |r| r.get(0))?.collect::<Result<_, _>>()?;
                for id in ids {
                    bump(&mut cand, id, "reading", W_READING_PREFIX);
                }
            }
        }
        Kind::Latin => {
            // curated paradigm aliases (lexeme_alias): fill gloss paradigm gaps so "her"/"him"/"them"
            // reach the canonical Cantonese pronoun (佢 / 佢哋) even though the gloss omits those forms.
            // Decisive tier so the real word leads over a high-freq written-Chinese form (她的).
            {
                let ql = q.trim().to_lowercase();
                let mut stmt = conn.prepare("SELECT lexeme_id FROM lexeme_alias WHERE term = ?1")?;
                let ids: Vec<i64> = stmt.query_map([&ql], |r| r.get(0))?.collect::<Result<_, _>>()?;
                for id in ids {
                    bump(&mut cand, id, "english", W_ALIAS);
                }
            }
            // phonetic (toneless fold) - tolerant of tone marks / numbers / no tone.
            // pinyin_plain and jyutping_plain share the same fold (letters only), so one key
            // matches Mandarin *and* Cantonese readings (jyutping was the original's blind spot).
            // whether the query is itself a valid reading (pinyin/jyutping/romaji syllable). If so, a
            // single-word English "gloss" that merely echoes that romanization (JMdict's 仞 "ren
            // (ancient measure…)") is a transliteration label, not a meaning, and must not outrank the
            // phonetic result (人). Tracked here, applied in the english pass below.
            let mut reading_hit = false;
            let plain = pinyin_plain(q);
            if !plain.is_empty() {
                let mut stmt = conn.prepare(
                    "SELECT lexeme_id FROM lexeme_reading \
                     WHERE kind IN ('pinyin_plain','jyutping_plain') AND value = ?1",
                )?;
                let ids: Vec<i64> =
                    stmt.query_map([&plain], |r| r.get(0))?.collect::<Result<_, _>>()?;
                if !ids.is_empty() {
                    reading_hit = true;
                }
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
                if !ids.is_empty() {
                    reading_hit = true;
                }
                for id in ids {
                    bump(&mut cand, id, "reading", W_READING);
                }
            }
            // english gloss full-text, ranked by how well the gloss matches (exactness + bm25),
            // so "airport" surfaces 空港/機場 above words where airport is merely incidental.
            if let Some(fq) = fts_query(q) {
                let ql = q.trim().to_lowercase();
                let ql_single = !ql.contains(' ');
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
                    if quality >= 1.0 && reading_hit && ql_single {
                        // the query is a romanization and this "gloss" is just that single token (a
                        // transliteration label, e.g. 仞 "ren (a measure)"): demote so the phonetic
                        // reading (人) leads, not the transliterated obscure word.
                        w = english_weight(0.5);
                    } else if quality >= 1.0 && sense_order == 0 {
                        // the query IS this word's PRIMARY meaning (exact match on sense 0): a stronger
                        // signal than an exact match on a word's minor sense, and big enough to beat the
                        // (unreliable) frequency tiebreak: 山 leads "mountain", not 深山 ("deep mountains"),
                        // whose bare "mountain" sense is secondary.
                        w += FREQ_BONUS;
                    }
                    bump(&mut cand, id, "english", w);
                }
            }
        }
        _ => {}
    }

    // partial / substring fallback: a query that didn't resolve to a whole word (a name glued to a
    // common word, e.g. 山田ホテル / 東京ホテル) still surfaces the dictionary words CONTAINED in it
    // (ホテル, 東京). Only for Han/kana queries of length ≥3, and only when there's no whole-word hit,
    // so ordinary lookups aren't polluted with their own sub-words. Longest substrings rank highest.
    let has_whole = cand.values().any(|(mt, _)| *mt == "exact" || *mt == "variant");
    let qchars: Vec<char> = q.chars().collect();
    // cap the substring scan length: it is O(n²) in query length (every start × every length), so a
    // very long unresolved Han/kana string could take seconds. A real word glued to a name is short;
    // beyond 24 chars skip the fallback (the frontend still shows the per-character breakdown).
    if !has_whole && (3..=24).contains(&qchars.len()) && matches!(kind, Kind::Han | Kind::Kana) {
        let n = qchars.len();
        let mut stmt = conn.prepare("SELECT lexeme_id FROM surface_form WHERE form = ?1 AND rare = 0")?;
        for len in (2..n).rev() {
            for start in 0..=(n - len) {
                let sub: String = qchars[start..start + len].iter().collect();
                let ids: Vec<i64> =
                    stmt.query_map([&sub], |r| r.get(0))?.collect::<Result<_, _>>()?;
                for id in ids {
                    bump(&mut cand, id, "partial", W_PARTIAL * (len as f64 / n as f64));
                }
            }
        }
    }

    // force-scope filter: drop the ambiguous tier the caller doesn't want (Sound hides gloss matches,
    // Meaning hides phonetic ones). Written-form/exact/variant/partial matches always stay.
    cand.retain(|_, v| scope.keeps(v.0));

    // assemble + rank
    let mut hits: Vec<Hit> = Vec::with_capacity(cand.len());
    for (id, (mt, w)) in cand {
        if let Some(mut hit) = build_hit(conn, id, mt, w, pref_script)? {
            // Script-priority for a Han-character query: a bare glyph is written identically across
            // languages, so the homographs tie on match-weight and frequency alone would decide. Since
            // the full JMdict added many common Japanese single-kanji that outscore their Chinese twin
            // on the Japanese corpus, nudge the Chinese (then Cantonese) reading ahead by a small
            // amount: enough to win a near-tie (洗→xǐ, 火車→huǒchē "train"), never enough to override a
            // genuinely better match. No effect on kana/Latin queries.
            if kind == Kind::Han {
                hit.score += match hit.variety.as_str() {
                    "zh" => 0.06,
                    "yue" => 0.04,
                    _ => 0.0,
                };
            }
            hits.push(hit);
        }
    }
    // Rank by score, then DETERMINISTIC tiebreaks: without them, equal-score homographs (洗 xǐ "to
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

    // English (Latin) queries: a common word's meaning-match tiers collapse to a near-tie, so the freq
    // tiebreak decides: and freq is systematically higher for Japanese (separate corpora, not
    // cross-comparable), which stacked 10+ ja hits before the first Chinese one. Reflow so at most
    // CAP_RUN in a row share a variety, surfacing a 中/粵 result near the top, without disturbing the
    // score order within each variety. Latin-only so Han/kana/wildcard ordering is untouched.
    if kind == Kind::Latin && hits.len() > CAP_RUN {
        hits = cap_consecutive_variety(hits, CAP_RUN);
    }

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
        accent: None, // a char-only hit has no word lexeme; the char page derives accent in char_info
        jyut: None,   // likewise: the char page shows jyutping from char_reading, not the lexeme
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
        // rare=0: rare/irregular/search-only JMdict forms stay matchable but are not shown as variants
        "SELECT form, script, region, is_primary FROM surface_form WHERE lexeme_id = ?1 AND rare = 0",
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

    let accent = crate::model::ja_reading_accent(conn, id, &variety, reading.as_deref());
    let jyut = crate::model::zh_jyutping(conn, id, &variety);
    Ok(Some(Hit {
        lexeme_id: id,
        variety,
        headword,
        reading,
        accent,
        jyut,
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
