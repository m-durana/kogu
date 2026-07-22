//! Response shapes for the JSON API.

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct SearchResponse {
    pub query: String,
    pub classified_as: String,
    pub results: Vec<Hit>,
}

/// /interesting response: a fresh-random showcase of noteworthy entries for the homepage
/// (kokuji, false friends, words coined in Japan, surprising loanwords, calques). Each item
/// carries a short `why` label explaining what makes it interesting.
#[derive(Serialize, ToSchema)]
pub struct InterestingResponse {
    pub items: Vec<InterestingItem>,
}

#[derive(Serialize, Clone, ToSchema)]
pub struct InterestingItem {
    pub lexeme_id: i64,
    pub variety: String, // zh | yue | ja
    pub headword: String,
    pub reading: Option<String>,
    pub gloss: Option<String>,
    /// short human label for what makes this entry interesting ("国字 · a kanji invented in Japan")
    pub why: String,
    /// machine key for the category: kokuji | false-friend | wasei | cantoji | merge | english-false-friend
    pub category: String,
}

/// /suggest response: lightweight autocomplete candidates (no senses), fast per-keystroke.
#[derive(Serialize, ToSchema)]
pub struct SuggestResponse {
    pub query: String,
    pub suggestions: Vec<SuggestItem>,
}

#[derive(Serialize, ToSchema)]
pub struct SuggestItem {
    pub headword: String,
    pub reading: Option<String>,
    pub variety: String,
}

#[derive(Serialize, ToSchema)]
pub struct Hit {
    pub lexeme_id: i64,
    pub variety: String,        // zh | yue | ja
    pub headword: String,
    pub reading: Option<String>,
    /// Japanese pitch accent (Kanjium) for the kana reading, ja only; drives the synth + contour even
    /// when this word is shown as a cross-listed row under another variety's entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    /// Cantonese reading (jyutping) for a zh word, so the 粵 reading shows on the Chinese row even
    /// when the word is rendered from a search hit rather than its full entry (Cantonese shares the
    /// written form; only the pronunciation differs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jyut: Option<String>,
    pub forms: Vec<Form>,
    pub glosses: Vec<String>,
    pub match_type: String,     // exact | variant | reading | english
    pub score: f64,
}

#[derive(Serialize, Clone, ToSchema)]
pub struct Form {
    pub form: String,
    pub script: String,
    pub region: Option<String>,
    pub is_primary: bool,
}

#[derive(Serialize, ToSchema)]
pub struct Entry {
    pub lexeme_id: i64,
    pub variety: String,
    pub headword: String,
    pub reading: Option<String>,
    pub freq: Option<f64>,
    pub forms: Vec<Form>,
    pub readings: Vec<ReadingKV>,
    pub senses: Vec<Sense>,
    pub characters: Vec<CharInfo>,
    /// 同字 - other lexemes sharing this word's backbone form, each labelled cognate / false-friend.
    pub same_form: Vec<LinkLite>,
    /// 同義 - lexemes sharing a concept (a different word, same meaning) across the systems.
    pub translations: Vec<LinkLite>,
    /// 熟語 - common words that contain this character (single-character entries only).
    pub compounds: Vec<LinkLite>,
    /// lexical "why": origin badges (wasei-kango, borrowed-from-japanese, calque, …) - no LLM.
    pub origin_badges: Vec<String>,
    /// Wiktionary etymology paragraph for the looked-up lexeme, passthrough (kept for back-compat).
    pub etymology: Option<String>,
    /// Per-language origin accounts (中 Sinitic, 日 Japonic, …) for the SAME glyph. The Chinese and
    /// Japanese etymologies of 山 are both true and complementary; we surface each, labelled by
    /// variety, instead of silently showing whichever lexeme happened to rank first.
    pub origins: Vec<OriginAccount>,
    /// For a radical/bound-component entry (彳, 辵, 氵…): the characters that CONTAIN it, replacing the
    /// word "used in" list (a radical isn't a morpheme in words). Empty for ordinary entries.
    pub appears_in: Vec<CharLite>,
}

#[derive(Serialize, ToSchema)]
pub struct OriginAccount {
    pub variety: String,
    pub headword: String,
    pub text: String,
    /// pre-baked English machine translation, when the etymology text is not already English
    /// (Chinese idiom 出處, native zh/ja Wiktionary 詞源/語源). None when text is English or untranslated.
    pub text_en: Option<String>,
    /// which script this glyph is, when it diverges across reforms: "traditional" | "simplified".
    /// None when the glyph is the same in every script (山, 古): nothing to disambiguate.
    pub script: Option<String>,
    /// a clarifying note when this glyph ALSO doubles as the simplified form of a distinct character
    /// (丑 is the earthly branch AND the simplified form of 醜 "ugly"). Prevents the reader from
    /// thinking the origin paragraph below describes the merged-in traditional character.
    pub note: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CharLite {
    pub ch: String,
    pub gloss: Option<String>,
    /// true for rare extension-plane glyphs (cp ≥ U+20000) the device font likely renders as tofu;
    /// the UI de-emphasises these and labels them with their codepoint rather than a blank box.
    pub rare: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ReadingKV {
    pub kind: String,
    pub value: String,
    /// Japanese pitch accent (Kanjium, CC BY-SA 4.0) on a ja kind='kana' reading: the downstep mora
    /// index as a string ("0"=heiban, "1"=atamadaka, n=drop after mora n; a multi-accent word keeps
    /// the comma list "2,1"). Omitted (null) for every reading without Kanjium accent data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct Sense {
    pub pos: Option<String>,
    pub gloss_en: String,
}

#[derive(Serialize, ToSchema)]
pub struct CharInfo {
    pub ch: String,
    pub is_orthodox: bool,
    pub strokes: Option<i64>,
    pub radical: Option<i64>,
    pub ids: Option<String>,
    pub gloss_en: Option<String>,
    /// Kanjidic Japanese-perspective English meaning (津 → "haven; port; harbor"), distinct from the
    /// Chinese-centric Unihan gloss_en. Present only for characters in actual Japanese use.
    pub gloss_ja: Option<String>,
    pub readings: Vec<ReadingKV>,
    /// orthographic "why" seed: identity edges to orthodox parents with the reform that produced them
    pub variants: Vec<VariantEdge>,
    /// the character's script family across reforms (繁→简·日), for the forms strip. None when the
    /// glyph has no living cross-script branches and isn't a kokuji (nothing to show).
    pub script_forms: Option<ScriptForms>,
    /// confusable look-alikes (Unihan kSpoofingVariant): glyphs easily MISREAD for this one (㓕/滅).
    /// Purely a visual-confusability note, NOT identity or meaning; empty for most characters.
    pub confusables: Vec<String>,
    /// when the character is built entirely from repetitions of ONE simpler glyph (森 = three 木,
    /// 晶 = three 日, 淼 = three 水), resolved recursively through "doubled" intermediates (林, 昍, 沝).
    /// None for mixed-component characters (好 = 女 + 子): the frontend then shows the flat parts.
    pub decomp: Option<CharDecomp>,
    /// the character's distinct components WITH their meanings (森 → 木 "tree"; 好 → 女 "woman", 子
    /// "child"), so the structure section explains the parts, not just lists them. Radical-variant
    /// forms are glossed via their parent character (亻 → "person").
    pub components: Vec<Component>,
    /// true when this glyph is primarily a Kangxi RADICAL / bound component, not a standalone word
    /// (彳, 辵, 氵, 艹…). Detected from a radical-flagging gloss AND near-zero containing words.
    pub is_radical: bool,
    /// the Kangxi radical number when known (parsed from the gloss), for the radical line.
    pub radical_number: Option<i64>,
    /// the standalone character a radical-variant form stands for (氵→水, 辶→辵), when it differs.
    pub standalone: Option<String>,
    /// how many lexemes contain this character (a usage signal): 0 = archaic/unused, large = core.
    /// Global across languages; used for radical detection (a radical is bound in every language).
    pub used_count: i64,
    /// per-language containing-word counts ({"zh":449,"yue":57,"ja":7}), so the "rarely used" tag can
    /// be language-specific (巴 is common in Chinese but rare in Japanese). Absent varieties = 0.
    pub used_by_variety: std::collections::HashMap<String, i64>,
    /// per-language MAX word-frequency (wordfreq Zipf score, 0..1) among words containing this glyph.
    /// The real rarity signal that drives the "rarely used"/"uncommon" tag; a count mislabels common
    /// particles (嗎/也). Absent variety = no scored word in that language.
    pub freq_by_variety: std::collections::HashMap<String, f64>,
    /// ancient-script periods with an image for this glyph, in chronological order, a subset of
    /// ["oracle","bronze","seal"] (甲骨文/金文/篆書). Each is served at /ancient/{cp}/{period}. Empty
    /// when Commons has no ancient form for the character.
    pub ancient: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct Component {
    pub ch: String,
    pub gloss: Option<String>,
    /// 'semantic' (carries the meaning) | 'phonetic' (carries the sound) | 'form' | 'iconic' | None.
    /// From Wiktionary's structured Han-compound data: 媽 = 女 (semantic) + 馬 (phonetic).
    pub role: Option<String>,
    /// for a phonetic component, the sound it lends: the component's own reading (馬 → "ma3"), so the
    /// UI can show "(sound: mǎ)". None for non-phonetic components or when no reading is known.
    pub sound: Option<String>,
    /// for a phonetic component, its Middle Chinese (廣韻 / Baxter) reading(s): the HISTORICAL sound
    /// it lent (同 → "duwng"), so the structure section can show the phonological "why": the modern
    /// pinyin link plus the older Middle Chinese one. Empty for non-phonetic components or no MC data.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mc_sound: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CharDecomp {
    pub base: String,
    pub count: i64,
}

#[derive(Serialize, ToSchema)]
pub struct VariantEdge {
    pub parent: String,
    pub edge_type: String,
    pub reform: Option<String>,
    pub reform_name: Option<String>,
    pub reform_year: Option<i64>,
}

/// The script forms of one character family, anchored on the orthodox glyph. Branches include the
/// orthodox form itself plus its living simplified/shinjitai/z-variant children.
#[derive(Serialize, ToSchema)]
pub struct ScriptForms {
    pub orthodox: String,
    pub is_kokuji: bool,
    pub branches: Vec<FormBranch>,
}

#[derive(Serialize, ToSchema)]
pub struct FormBranch {
    pub form: String,
    pub script: String, // "traditional" | "simplified" | "shinjitai" | "z-variant"
    pub reform_id: Option<String>,
    pub reform_label: Option<String>, // plain-language: "PRC simplification", "Tōyō shinjitai", …
    pub is_orthodox: bool,
}

/// /why response - the orthographic + phonological "why" for a word (DESIGN.md §4).
#[derive(Serialize, ToSchema)]
pub struct WhyResponse {
    pub lexeme_id: i64,
    pub headword: String,
    pub characters: Vec<CharInfo>,
}

/// Kanjium pitch accent for a Japanese lexeme's kana reading (None for non-ja or when absent). Prefers
/// the kana that equals the lexeme's primary `reading`, so a homograph gets its own word's accent. Used
/// to give cross-listed ja rows (same_form / hit) the same forced accent + contour as a direct lookup.
pub fn ja_reading_accent(
    conn: &rusqlite::Connection,
    lexeme_id: i64,
    variety: &str,
    reading: Option<&str>,
) -> Option<String> {
    if variety != "ja" {
        return None;
    }
    conn.query_row(
        "SELECT accent FROM lexeme_reading WHERE lexeme_id=?1 AND kind='kana' AND accent IS NOT NULL \
         ORDER BY (value = ?2) DESC LIMIT 1",
        rusqlite::params![lexeme_id, reading],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

/// Cantonese reading (jyutping) for a zh word lexeme, None for other varieties or when absent.
/// Cantonese shares the written form with Chinese, so a zh word's 粵 pronunciation belongs on the
/// same row wherever the word appears (hit, same_form link), not only on its full entry.
pub fn zh_jyutping(conn: &rusqlite::Connection, lexeme_id: i64, variety: &str) -> Option<String> {
    if variety != "zh" {
        return None;
    }
    conn.query_row(
        "SELECT value FROM lexeme_reading WHERE lexeme_id=?1 AND kind='jyutping' LIMIT 1",
        [lexeme_id],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

#[derive(Serialize, ToSchema)]
pub struct LinkLite {
    pub lexeme_id: i64,
    pub variety: String,
    pub headword: String,
    pub reading: Option<String>,
    /// Japanese pitch accent (Kanjium) for the kana reading, ja only (see [`Hit::accent`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    /// Cantonese reading (jyutping) for a zh word (see [`Hit::jyut`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jyut: Option<String>,
    pub glosses: Vec<String>,
    /// relation to the anchor word: "cognate" | "false-friend" | "synonym"
    pub relation: String,
    /// the shared concept label (for 同義 links), when known
    pub concept: Option<String>,
}

/// /translate response: an English term → concepts → equivalents across all systems.
#[derive(Serialize, ToSchema)]
pub struct TranslateResponse {
    pub query: String,
    pub concepts: Vec<ConceptGroup>,
}

#[derive(Serialize, ToSchema)]
pub struct ConceptGroup {
    pub concept: String,
    pub members: Vec<LinkLite>,
}

/// /segment response: an unrecognized Han query greedily split into the longest known sub-words, each
/// with a short gloss, so the "literally" hint reads 紅出口 → "red · exit" instead of "red · go out · mouth".
#[derive(Serialize, ToSchema)]
pub struct SegmentResponse {
    pub query: String,
    pub segments: Vec<SegmentPart>,
}

#[derive(Serialize, ToSchema)]
pub struct SegmentPart {
    /// the matched sub-word (one or more characters)
    pub form: String,
    /// its short gloss (first cleaned sense segment); empty if nothing is known for the character
    pub gloss: String,
    /// the lexeme this gloss came from, when the segment is a known word (null for a character fallback)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lexeme_id: Option<i64>,
}

/// /ocr response - recognized text laid out over the image for tap-to-select (DESIGN: OCR feature).
#[derive(Serialize, ToSchema)]
pub struct OcrResponse {
    /// the (possibly downscaled) image dimensions the boxes are in
    pub width: u32,
    pub height: u32,
    pub lines: Vec<OcrLine>,
}

#[derive(Serialize, ToSchema)]
pub struct OcrLine {
    pub text: String,
    pub confidence: f32,
    /// axis-aligned bounding box [x, y, w, h] in image pixels
    #[serde(rename = "box")]
    pub box_: [f32; 4],
    /// per-character cells (line box split by character count - Han is ~monospace)
    pub chars: Vec<OcrChar>,
}

/// Error body shape used by every JSON endpoint (400 / 404 / 5xx): `{ "error": "<code or message>" }`.
/// Documentation-only: handlers build it with `serde_json::json!`.
#[derive(Serialize, ToSchema)]
pub struct ApiError {
    /// short machine-readable code ("not_found", "bad_image") or an error message
    #[schema(example = "not_found")]
    pub error: String,
}

/// Liveness probe body (documentation-only; the handler builds it with `serde_json::json!`).
#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    #[schema(example = "ok")]
    pub status: String,
    #[schema(example = "kogu")]
    pub service: String,
    #[schema(example = "1.0.0")]
    pub version: String,
}

/// `/random` body: the id of a random common lexeme, to open through `/entry/{id}`.
#[derive(Serialize, ToSchema)]
pub struct RandomResponse {
    #[schema(example = 8502)]
    pub lexeme_id: i64,
}

#[derive(Serialize, ToSchema)]
pub struct OcrChar {
    pub ch: String,
    #[serde(rename = "box")]
    pub box_: [f32; 4],
}
