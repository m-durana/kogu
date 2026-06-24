//! Japanese text-to-speech proxy. The browser's SpeechSynthesis voice cannot honour Japanese pitch
//! accent (it flattens or guesses it), so a ja reading is synthesized by a small local OpenJTalk
//! service (pyopenjtalk) that FORCES the stored Kanjium downstep, returning a short mp3. This handler
//! is a thin proxy to that loopback service, which owns the on-disk mp3 cache; the frontend falls back
//! to SpeechSynthesis when this is unavailable (offline / service down).

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::state::AppState;

// Per-syllable pronunciation clips for Mandarin (numbered-pinyin) and Cantonese (jyutping). The
// frontend used to fetch these straight from the upstream CDNs, but cross-origin CDNs are unreachable
// for some users (mainland China blocks jsDelivr; some iOS/PWA setups fail the cross-origin fetch),
// which made zh/yue audio silently fall back to a (usually missing) OS voice. We proxy them so every
// clip is SAME-ORIGIN, exactly like the Japanese /tts/ja path.
const ZH_BASE: &str = "https://cdn.jsdelivr.net/gh/davinfifield/mp3-chinese-pinyin-sound@master/mp3/";
const YUE_BASE: &str = "https://jyutping.org/audio/";

/// `GET /clip/:variety/:file` — proxy a single zh/yue pronunciation clip, cached in memory and served
/// same-origin. `file` is a strict syllable (lowercase letters + a tone digit) + ".mp3" so this can't
/// be turned into an open proxy / SSRF.
pub async fn clip_handler(
    State(st): State<AppState>,
    Path((variety, file)): Path<(String, String)>,
) -> impl IntoResponse {
    let base = match variety.as_str() {
        "zh" => ZH_BASE,
        "yue" => YUE_BASE,
        _ => return (StatusCode::NOT_FOUND, "unknown variety").into_response(),
    };
    // strict whitelist: "<letters><tone-digit>.mp3" (zh tones 1-5, yue 1-6) — nothing else.
    let syl = match file.strip_suffix(".mp3") {
        Some(s) => s,
        None => return (StatusCode::BAD_REQUEST, "bad clip").into_response(),
    };
    let ok = !syl.is_empty()
        && syl.len() <= 12
        && syl
            .chars()
            .enumerate()
            .all(|(i, c)| if i + 1 == syl.len() { c.is_ascii_digit() } else { c.is_ascii_lowercase() })
        && syl.chars().next().is_some_and(|c| c.is_ascii_lowercase());
    if !ok {
        return (StatusCode::BAD_REQUEST, "bad clip").into_response();
    }

    let key = format!("{variety}/{syl}");
    if let Some(bytes) = st.clip_cache.lock().unwrap().get(&key).cloned() {
        return clip_ok(bytes);
    }
    let url = format!("{base}{syl}.mp3");
    match st.http.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let ctype = resp.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            // jyutping.org answers a missing syllable with a 200 text/html SPA page — accept only audio.
            if !ctype.starts_with("audio") {
                return (StatusCode::NOT_FOUND, "no clip").into_response();
            }
            match resp.bytes().await {
                Ok(b) => {
                    let bytes = b.to_vec();
                    let mut c = st.clip_cache.lock().unwrap();
                    if c.len() < 6000 {
                        c.insert(key, bytes.clone());
                    }
                    clip_ok(bytes)
                }
                Err(_) => (StatusCode::BAD_GATEWAY, "clip read failed").into_response(),
            }
        }
        Ok(_) => (StatusCode::NOT_FOUND, "no clip").into_response(),
        Err(_) => (StatusCode::BAD_GATEWAY, "clip upstream failed").into_response(),
    }
}

fn clip_ok(bytes: Vec<u8>) -> axum::response::Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "audio/mpeg"),
            (header::CACHE_CONTROL, "public, max-age=2592000, immutable"),
        ],
        bytes,
    )
        .into_response()
}

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
