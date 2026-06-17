//! Response shapes for the JSON API.

use serde::Serialize;

#[derive(Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub classified_as: String,
    pub results: Vec<Hit>,
}

#[derive(Serialize)]
pub struct Hit {
    pub lexeme_id: i64,
    pub variety: String,        // zh | yue | ja
    pub headword: String,
    pub reading: Option<String>,
    pub forms: Vec<Form>,
    pub glosses: Vec<String>,
    pub match_type: String,     // exact | variant | reading | english
    pub score: f64,
}

#[derive(Serialize, Clone)]
pub struct Form {
    pub form: String,
    pub script: String,
    pub region: Option<String>,
    pub is_primary: bool,
}

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct OriginAccount {
    pub variety: String,
    pub headword: String,
    pub text: String,
}

#[derive(Serialize)]
pub struct CharLite {
    pub ch: String,
    pub gloss: Option<String>,
}

#[derive(Serialize)]
pub struct ReadingKV {
    pub kind: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct Sense {
    pub pos: Option<String>,
    pub gloss_en: String,
}

#[derive(Serialize)]
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
    /// when the character is built entirely from repetitions of ONE simpler glyph (森 = three 木,
    /// 晶 = three 日, 淼 = three 水), resolved recursively through "doubled" intermediates (林, 昍, 沝).
    /// None for mixed-component characters (好 = 女 + 子) — the frontend then shows the flat parts.
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
    pub used_count: i64,
}

#[derive(Serialize)]
pub struct Component {
    pub ch: String,
    pub gloss: Option<String>,
    /// 'semantic' (carries the meaning) | 'phonetic' (carries the sound) | 'form' | 'iconic' | None.
    /// From Wiktionary's structured Han-compound data — 媽 = 女 (semantic) + 馬 (phonetic).
    pub role: Option<String>,
    /// for a phonetic component, the sound it lends — the component's own reading (馬 → "ma3"), so the
    /// UI can show "(sound: mǎ)". None for non-phonetic components or when no reading is known.
    pub sound: Option<String>,
}

#[derive(Serialize)]
pub struct CharDecomp {
    pub base: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct VariantEdge {
    pub parent: String,
    pub edge_type: String,
    pub reform: Option<String>,
    pub reform_name: Option<String>,
    pub reform_year: Option<i64>,
}

/// The script forms of one character family, anchored on the orthodox glyph. Branches include the
/// orthodox form itself plus its living simplified/shinjitai/z-variant children.
#[derive(Serialize)]
pub struct ScriptForms {
    pub orthodox: String,
    pub is_kokuji: bool,
    pub branches: Vec<FormBranch>,
}

#[derive(Serialize)]
pub struct FormBranch {
    pub form: String,
    pub script: String, // "traditional" | "simplified" | "shinjitai" | "z-variant"
    pub reform_id: Option<String>,
    pub reform_label: Option<String>, // plain-language: "PRC simplification", "Tōyō shinjitai", …
    pub is_orthodox: bool,
}

/// /why response - the orthographic + phonological "why" for a word (DESIGN.md §4).
#[derive(Serialize)]
pub struct WhyResponse {
    pub lexeme_id: i64,
    pub headword: String,
    pub characters: Vec<CharInfo>,
}

#[derive(Serialize)]
pub struct LinkLite {
    pub lexeme_id: i64,
    pub variety: String,
    pub headword: String,
    pub reading: Option<String>,
    pub glosses: Vec<String>,
    /// relation to the anchor word: "cognate" | "false-friend" | "synonym"
    pub relation: String,
    /// the shared concept label (for 同義 links), when known
    pub concept: Option<String>,
}

/// /translate response: an English term → concepts → equivalents across all systems.
#[derive(Serialize)]
pub struct TranslateResponse {
    pub query: String,
    pub concepts: Vec<ConceptGroup>,
}

#[derive(Serialize)]
pub struct ConceptGroup {
    pub concept: String,
    pub members: Vec<LinkLite>,
}

/// /ocr response - recognized text laid out over the image for tap-to-select (DESIGN: OCR feature).
#[derive(Serialize)]
pub struct OcrResponse {
    /// the (possibly downscaled) image dimensions the boxes are in
    pub width: u32,
    pub height: u32,
    pub lines: Vec<OcrLine>,
}

#[derive(Serialize)]
pub struct OcrLine {
    pub text: String,
    pub confidence: f32,
    /// axis-aligned bounding box [x, y, w, h] in image pixels
    #[serde(rename = "box")]
    pub box_: [f32; 4],
    /// per-character cells (line box split by character count - Han is ~monospace)
    pub chars: Vec<OcrChar>,
}

#[derive(Serialize)]
pub struct OcrChar {
    pub ch: String,
    #[serde(rename = "box")]
    pub box_: [f32; 4],
}
