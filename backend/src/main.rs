//! kogu serving binary — read-only dictionary API (DESIGN.md §7).
//!
//! Loads the precompiled SQLite DB (read-only, mmap'd) and the in-memory variant graph once at
//! startup; every request is just index hits. No heavy work on the serving path.

use std::net::SocketAddr;

use kogu::{build_router, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kogu=info,tower_http=info".into()),
        )
        .init();

    let db_path = std::env::var("KOGU_DB").unwrap_or_else(|_| "../data/kogu.sqlite".into());
    tracing::info!("loading {db_path}");
    let st = AppState::load(&db_path)?;
    tracing::info!("variant graph loaded: {} backbone keys indexed", st.graph.num_classes());

    let app = build_router(st);

    let port: u16 = std::env::var("KOGU_PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(8080);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("kogu listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
