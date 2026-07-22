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

/// `GET /clip/:variety/:file`: proxy a single zh/yue pronunciation clip, cached in memory and served
/// same-origin. `file` is a strict syllable (lowercase letters + a tone digit) + ".mp3" so this can't
/// be turned into an open proxy / SSRF.
#[utoipa::path(
    get, path = "/clip/{variety}/{file}", tag = "audio",
    params(
        ("variety" = String, Path, description = "\"zh\" (Mandarin, numbered pinyin) or \"yue\" (Cantonese, jyutping)"),
        ("file" = String, Path, description = "syllable clip name: lowercase letters + tone digit + \".mp3\" (e.g. ma3.mp3, jyut6.mp3)"),
    ),
    responses(
        (status = 200, description = "MP3 clip (cacheable, immutable)", content_type = "audio/mpeg", body = Vec<u8>),
        (status = 400, description = "Malformed clip name (plain-text message)"),
        (status = 404, description = "Unknown variety or no such syllable (plain-text message)"),
        (status = 502, description = "Upstream clip source unreachable (plain-text message)"),
    )
)]
pub async fn clip_handler(
    State(st): State<AppState>,
    Path((variety, file)): Path<(String, String)>,
) -> impl IntoResponse {
    let base = match variety.as_str() {
        "zh" => ZH_BASE,
        "yue" => YUE_BASE,
        _ => return (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"error": "unknown variety"}))).into_response(),
    };
    // strict whitelist: "<letters><tone-digit>.mp3" (zh tones 1-5, yue 1-6): nothing else.
    let syl = match file.strip_suffix(".mp3") {
        Some(s) => s,
        None => return (StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({"error": "bad clip"}))).into_response(),
    };
    let ok = !syl.is_empty()
        && syl.len() <= 12
        && syl
            .chars()
            .enumerate()
            .all(|(i, c)| if i + 1 == syl.len() { c.is_ascii_digit() } else { c.is_ascii_lowercase() })
        && syl.chars().next().is_some_and(|c| c.is_ascii_lowercase());
    if !ok {
        return (StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({"error": "bad clip"}))).into_response();
    }

    let key = format!("{variety}/{syl}");
    if let Some(bytes) = st.clip_cache.lock().unwrap().get(&key).cloned() {
        return clip_ok(bytes);
    }
    let url = format!("{base}{syl}.mp3");
    match st.http.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let ctype = resp.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            // jyutping.org answers a missing syllable with a 200 text/html SPA page: accept only audio.
            if !ctype.starts_with("audio") {
                return (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"error": "no clip"}))).into_response();
            }
            match resp.bytes().await {
                Ok(b) => {
                    // normalize to the same EBU R128 target as the ja synth (tts/synth_service.py):
                    // the raw sources differ wildly (zh clips ~-35 dBFS RMS vs synth ~-19), which
                    // made tapping speakers across languages jump in volume. ffmpeg failure falls
                    // back to the original clip: worse loudness beats no audio.
                    let bytes = normalize_clip(b.to_vec()).await;
                    let mut c = st.clip_cache.lock().unwrap();
                    if c.len() < 6000 {
                        c.insert(key, bytes.clone());
                    }
                    clip_ok(bytes)
                }
                Err(_) => (StatusCode::BAD_GATEWAY, axum::Json(serde_json::json!({"error": "clip read failed"}))).into_response(),
            }
        }
        Ok(_) => (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"error": "no clip"}))).into_response(),
        Err(_) => (StatusCode::BAD_GATEWAY, axum::Json(serde_json::json!({"error": "clip upstream failed"}))).into_response(),
    }
}

/// Loudness-normalize an mp3 clip via ffmpeg (loudnorm, same target as the ja TTS sidecar).
/// Any failure returns the ORIGINAL bytes.
async fn normalize_clip(bytes: Vec<u8>) -> Vec<u8> {
    use tokio::io::AsyncWriteExt;
    let spawned = tokio::process::Command::new("ffmpeg")
        .args([
            "-loglevel", "error", "-f", "mp3", "-i", "pipe:0",
            "-af", "loudnorm=I=-23:TP=-2:LRA=11",
            "-codec:a", "libmp3lame", "-b:a", "64k", "-ac", "1", "-f", "mp3", "pipe:1",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut child) = spawned else { return bytes };
    let Some(mut stdin) = child.stdin.take() else { return bytes };
    let input = bytes.clone();
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(&input).await;
        // drop closes the pipe so ffmpeg sees EOF
    });
    match child.wait_with_output().await {
        Ok(out) if out.status.success() && !out.stdout.is_empty() => {
            let _ = writer.await;
            out.stdout
        }
        _ => {
            let _ = writer.await;
            bytes
        }
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

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TtsParams {
    /// the kana reading to speak (max 32 characters)
    kana: String,
    /// Kanjium downstep index ("0" = heiban .. n); optional: without it OpenJTalk picks its default.
    accent: Option<String>,
}

/// Local pyopenjtalk synth sidecar (see `tts/synth_service.py`, kogu-tts.service).
const SYNTH_URL: &str = "http://127.0.0.1:4120/synth";

/// Japanese TTS with forced pitch accent (OpenJTalk): a kana reading synthesized to a short MP3
/// honouring the Kanjium downstep, which browser SpeechSynthesis cannot do.
#[utoipa::path(
    get, path = "/tts/ja", tag = "audio",
    params(TtsParams),
    responses(
        (status = 200, description = "MP3 audio (cacheable per kana+accent)", content_type = "audio/mpeg", body = Vec<u8>),
        (status = 400, description = "Empty or over-long kana (plain-text message)"),
        (status = 502, description = "Synth sidecar unavailable (plain-text message)"),
    )
)]
pub async fn ja_handler(State(st): State<AppState>, Query(p): Query<TtsParams>) -> impl IntoResponse {
    let kana = p.kana.trim();
    // guard: a single reading is short; reject anything that isn't (keeps the synth fed clean input)
    if kana.is_empty() || kana.chars().count() > 32 {
        return (StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({"error": "bad kana"}))).into_response();
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
            Err(_) => (StatusCode::BAD_GATEWAY, axum::Json(serde_json::json!({"error": "synth read failed"}))).into_response(),
        },
        _ => (StatusCode::BAD_GATEWAY, axum::Json(serde_json::json!({"error": "synth unavailable"}))).into_response(),
    }
}
