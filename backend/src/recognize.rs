//! Handwriting recognition proxy (DESIGN.md §5.1).
//!
//! The backend proxies the Google Input Tools handwriting endpoint (the same engine Google
//! Translate uses) — image/shape based, so stroke-order-independent and "any-to-any". We never
//! send ink to the browser's origin; the Rust binary forwards it, dodging CORS and hiding the
//! client. PaddleOCR-via-`ort` is the future fallback behind this identical interface.
//!
//! `build_payload` and `parse_candidates` are pure so they can be unit-tested without a network.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::state::AppState;

const ENDPOINT: &str = "https://inputtools.google.com/request?ime=handwriting&app=kogu&dbg=0&cs=1&oe=UTF-8";

/// A stroke is an ordered list of [x, y, t] points (t = ms timestamp, may be 0).
type Point = [f64; 3];

#[derive(Deserialize)]
pub struct RecognizeRequest {
    pub width: f64,
    pub height: f64,
    pub strokes: Vec<Vec<Point>>,
    /// Recogniser language(s). Default ["zh"] (a Han superset). Pass e.g. ["zh","ja"] for any-to-any.
    #[serde(default)]
    pub languages: Vec<String>,
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct RecognizeResponse {
    pub candidates: Vec<String>,
    pub languages: Vec<String>,
}

/// Map our language tags to Google's handwriting language codes.
fn google_lang(tag: &str) -> &str {
    match tag {
        "zh" | "zh-CN" | "zh-Hans" => "zh",
        "zh_TW" | "zh-TW" | "zh-Hant" => "zh_TW",
        "ja" => "ja",
        "yue" | "zh-HK" => "yue-hant-t-i0-und", // Cantonese handwriting
        other => other,
    }
}

/// Build the Google Input Tools request body for a single language.
pub fn build_payload(width: f64, height: f64, strokes: &[Vec<Point>], language: &str) -> Value {
    // Google ink format: each stroke = [[x...], [y...], [t...]]
    let ink: Vec<Value> = strokes
        .iter()
        .map(|stroke| {
            let xs: Vec<f64> = stroke.iter().map(|p| p[0]).collect();
            let ys: Vec<f64> = stroke.iter().map(|p| p[1]).collect();
            let ts: Vec<f64> = stroke.iter().map(|p| p[2]).collect();
            json!([xs, ys, ts])
        })
        .collect();

    json!({
        "options": "enable_pre_space",
        "requests": [{
            "writing_guide": { "writing_area_width": width, "writing_area_height": height },
            "pre_context": "",
            "max_num_results": 10,
            "max_completions": 0,
            "ink": ink,
            "language": google_lang(language),
        }]
    })
}

/// Extract the ranked candidate list from a Google response value.
/// Shape: ["SUCCESS", [["<id>", ["c1","c2",...], [...], {...}]]]
pub fn parse_candidates(resp: &Value) -> Vec<String> {
    if resp.get(0).and_then(Value::as_str) != Some("SUCCESS") {
        return Vec::new();
    }
    resp.get(1)
        .and_then(|a| a.get(0))
        .and_then(|a| a.get(1))
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

pub async fn recognize_handler(
    State(st): State<AppState>,
    Json(req): Json<RecognizeRequest>,
) -> Result<Json<RecognizeResponse>, (StatusCode, Json<Value>)> {
    if req.strokes.is_empty() {
        return Ok(Json(RecognizeResponse { candidates: vec![], languages: vec![] }));
    }
    let langs = if req.languages.is_empty() { vec!["zh".to_string()] } else { req.languages.clone() };
    let limit = req.limit.unwrap_or(20).clamp(1, 50);

    // Query each language, merge candidates preserving order + dedup (any-to-any union set).
    let mut merged: Vec<String> = Vec::new();
    for lang in &langs {
        let payload = build_payload(req.width, req.height, &req.strokes, lang);
        let resp = st
            .http
            .post(ENDPOINT)
            .json(&payload)
            .send()
            .await
            .map_err(bad_gateway)?;
        let val: Value = resp.json().await.map_err(bad_gateway)?;
        for c in parse_candidates(&val) {
            if !merged.contains(&c) {
                merged.push(c);
            }
        }
    }
    merged.truncate(limit);
    Ok(Json(RecognizeResponse { candidates: merged, languages: langs }))
}

fn bad_gateway<E: std::fmt::Display>(e: E) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_GATEWAY, Json(json!({ "error": "recognizer_unavailable", "detail": e.to_string() })))
}
