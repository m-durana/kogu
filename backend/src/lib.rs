//! kogu library surface - shared by the binary and the integration tests.

pub mod graph;
pub mod handlers;
pub mod model;
pub mod ocr;
pub mod recognize;
pub mod search;
pub mod state;

use axum::{
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

use state::AppState;

/// Build the full application router for a loaded [`AppState`].
pub fn build_router(st: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/search", get(handlers::search_handler))
        .route("/entry/:id", get(handlers::entry_handler))
        .route("/recognize", post(recognize::recognize_handler))
        .route(
            "/ocr",
            post(ocr::ocr_handler).layer(axum::extract::DefaultBodyLimit::max(12 * 1024 * 1024)),
        )
        .route("/translate", get(handlers::translate_handler))
        .route("/why/:id", get(handlers::why_handler))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(st)
}

pub async fn not_implemented() -> (axum::http::StatusCode, Json<Value>) {
    (axum::http::StatusCode::NOT_IMPLEMENTED, Json(json!({ "error": "not_implemented" })))
}
