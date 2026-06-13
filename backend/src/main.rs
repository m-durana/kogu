//! kanzi serving binary.
//!
//! Read-only dictionary API. The full design lives in `docs/`. This file is the Phase-0
//! skeleton: it stands up the HTTP server and a `/health` route. The real endpoints
//! (`/search`, `/entry/{id}`, `/translate`, `/why/...`, `/recognize`) land in later phases —
//! their handlers are stubbed below so the route surface is visible from day one.

use axum::{routing::get, routing::post, Json, Router};
use serde_json::{json, Value};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kanzi=info,tower_http=info".into()),
        )
        .init();

    let app = Router::new()
        .route("/health", get(health))
        // --- Phase 0+: scored graph-expansion lookup ---
        .route("/search", get(not_implemented))
        .route("/entry/:id", get(not_implemented))
        // --- Phase 1: concept layer / English pivot ---
        .route("/translate", get(not_implemented))
        // --- Phase 2+: the "why" engine ---
        .route("/why/:id", get(not_implemented))
        // --- Phase 0: handwriting proxy ---
        .route("/recognize", post(not_implemented))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let port: u16 = std::env::var("KANZI_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    tracing::info!("kanzi listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "kanzi", "version": env!("CARGO_PKG_VERSION") }))
}

async fn not_implemented() -> (axum::http::StatusCode, Json<Value>) {
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        Json(json!({ "error": "not_implemented" })),
    )
}
