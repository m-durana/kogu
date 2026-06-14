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
    /// 同字 — other lexemes sharing this word's backbone form, each labelled cognate / false-friend.
    pub same_form: Vec<LinkLite>,
    /// 同義 — lexemes sharing a concept (a different word, same meaning) across the systems.
    pub translations: Vec<LinkLite>,
    /// lexical "why": origin badges (wasei-kango, borrowed-from-japanese, calque, …) — no LLM.
    pub origin_badges: Vec<String>,
    /// Wiktionary etymology paragraph, passthrough (no generated prose).
    pub etymology: Option<String>,
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
    pub readings: Vec<ReadingKV>,
    /// orthographic "why" seed: identity edges to orthodox parents with the reform that produced them
    pub variants: Vec<VariantEdge>,
}

#[derive(Serialize)]
pub struct VariantEdge {
    pub parent: String,
    pub edge_type: String,
    pub reform: Option<String>,
    pub reform_name: Option<String>,
    pub reform_year: Option<i64>,
}

/// /why response — the orthographic + phonological "why" for a word (DESIGN.md §4).
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

/// /ocr response — recognized text laid out over the image for tap-to-select (DESIGN: OCR feature).
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
    /// per-character cells (line box split by character count — Han is ~monospace)
    pub chars: Vec<OcrChar>,
}

#[derive(Serialize)]
pub struct OcrChar {
    pub ch: String,
    #[serde(rename = "box")]
    pub box_: [f32; 4],
}
