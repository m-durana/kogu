//! Japanese text-to-speech proxy. The browser's SpeechSynthesis voice cannot honour Japanese pitch
//! accent (it flattens or guesses it), so a ja reading is synthesized by a small local OpenJTalk
//! service (pyopenjtalk) that FORCES the stored Kanjium downstep, returning a short mp3. This handler
//! is a thin proxy to that loopback service, which owns the on-disk mp3 cache; the frontend falls back
//! to SpeechSynthesis when this is unavailable (offline / service down).

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct TtsParams {
    /// the kana reading to speak
    kana: String,
    /// Kanjium downstep index ("0" = heiban .. n); optional — without it OpenJTalk picks its default.
    accent: Option<String>,
}

/// Local pyopenjtalk synth sidecar (see `tts/synth_service.py`, kogu-tts.service).
const SYNTH_URL: &str = "http://127.0.0.1:4120/synth";

pub async fn ja_handler(State(st): State<AppState>, Query(p): Query<TtsParams>) -> impl IntoResponse {
    let kana = p.kana.trim();
    // guard: a single reading is short; reject anything that isn't (keeps the synth fed clean input)
    if kana.is_empty() || kana.chars().count() > 32 {
        return (StatusCode::BAD_REQUEST, "bad kana").into_response();
    }
    let mut query: Vec<(&str, &str)> = vec![("kana", kana)];
    if let Some(a) = p.accent.as_deref().filter(|a| !a.is_empty()) {
        query.push(("accent", a));
    }
    match st.http.get(SYNTH_URL).query(&query).send().await {
        Ok(resp) if resp.status().is_success() => match resp.bytes().await {
            Ok(bytes) => (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "audio/mpeg"),
                    // immutable per (kana, accent): safe to cache hard in the SW and the browser
                    (header::CACHE_CONTROL, "public, max-age=604800, immutable"),
                ],
                bytes,
            )
                .into_response(),
            Err(_) => (StatusCode::BAD_GATEWAY, "synth read failed").into_response(),
        },
        _ => (StatusCode::BAD_GATEWAY, "synth unavailable").into_response(),
    }
}
