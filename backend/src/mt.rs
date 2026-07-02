//! Machine-translation proxy for the in-app "Translate" panel. Calls the keyless Google endpoint
//! (translate_a/single, client=gtx — no token needed) server-side so the browser dodges CORS, with an
//! in-memory cache (the DB pool is read-only) and a MyMemory fallback on failure/rate-limit.

use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::state::AppState;

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MtParams {
    /// text to translate to English (max 200 characters)
    q: String,
    /// source language hint (zh-CN / zh-TW / ja / yue); defaults to auto-detect
    sl: Option<String>,
}

/// MyMemory needs an explicit source language; map our hints to its closest supported code.
fn mm_lang(sl: &str) -> &str {
    match sl {
        "ja" => "ja",
        "zh-TW" | "yue" => "zh-TW",
        _ => "zh-CN",
    }
}

async fn google_translate(http: &reqwest::Client, q: &str, sl: &str) -> Option<(String, Option<String>)> {
    let resp = http
        .get("https://translate.googleapis.com/translate_a/single")
        .query(&[("client", "gtx"), ("sl", sl), ("tl", "en"), ("dt", "t"), ("q", q)])
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    // v[0] = [ [translated, original, …], … ] sentence chunks; join the translated pieces. v[2] = detected src.
    let chunks = v.get(0)?.as_array()?;
    let mut out = String::new();
    for c in chunks {
        if let Some(s) = c.get(0).and_then(|x| x.as_str()) {
            out.push_str(s);
        }
    }
    if out.trim().is_empty() {
        return None;
    }
    let src = v.get(2).and_then(|x| x.as_str()).map(|s| s.to_string());
    Some((out, src))
}

async fn mymemory(http: &reqwest::Client, q: &str, sl: &str) -> Option<String> {
    let pair = format!("{}|en", mm_lang(sl));
    let resp = http
        .get("https://api.mymemory.translated.net/get")
        .query(&[("q", q), ("langpair", &pair), ("de", "real.akutasan@gmail.com")])
        .send()
        .await
        .ok()?;
    let v: Value = resp.json().await.ok()?;
    let t = v.get("responseData")?.get("translatedText")?.as_str()?.to_string();
    if t.trim().is_empty() {
        None
    } else {
        Some(t)
    }
}

/// Machine translation to English (sentence-level).
///
/// This endpoint PROXIES third-party services (Google translate_a/single, with a MyMemory
/// fallback); treat it as a convenience for the in-app "Translate" panel, not a stable data
/// endpoint. Always returns 200; failures are reported in the body as {"error": "translate_failed"}.
#[utoipa::path(
    get, path = "/mt", tag = "input",
    params(MtParams),
    responses((status = 200, description = "Translation (or an in-body error)", body = Value,
        example = json!({"translation": "hello", "source": "zh-CN", "engine": "google"})))
)]
pub async fn translate_handler(State(st): State<AppState>, Query(p): Query<MtParams>) -> Json<Value> {
    let q = p.q.trim().to_string();
    if q.is_empty() || q.chars().count() > 200 {
        return Json(json!({ "translation": "", "source": Value::Null }));
    }
    let sl = p.sl.unwrap_or_else(|| "auto".into());
    let key = format!("{sl}|{q}");

    if let Some((t, src)) = st.mt_cache.lock().unwrap().get(&key).cloned() {
        let source = if src.is_empty() { Value::Null } else { Value::String(src) };
        return Json(json!({ "translation": t, "source": source, "cached": true }));
    }

    let (translation, source, engine) = match google_translate(&st.http, &q, &sl).await {
        Some((t, s)) => (t, s, "google"),
        None => match mymemory(&st.http, &q, &sl).await {
            Some(t) => (t, None, "mymemory"),
            None => return Json(json!({ "error": "translate_failed" })),
        },
    };

    {
        let mut cache = st.mt_cache.lock().unwrap();
        if cache.len() > 5000 {
            cache.clear(); // crude bound; translations are cheap to refetch
        }
        cache.insert(key, (translation.clone(), source.clone().unwrap_or_default()));
    }
    let src = source.map(Value::String).unwrap_or(Value::Null);
    Json(json!({ "translation": translation, "source": src, "engine": engine }))
}
