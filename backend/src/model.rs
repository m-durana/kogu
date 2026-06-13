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
    /// 同字 — other lexemes sharing this word's backbone form (incl. cross-language false friends).
    pub same_form: Vec<LinkLite>,
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
}

#[derive(Serialize)]
pub struct LinkLite {
    pub lexeme_id: i64,
    pub variety: String,
    pub headword: String,
    pub glosses: Vec<String>,
}
