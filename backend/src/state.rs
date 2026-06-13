//! Shared application state: a small SQLite connection pool (read-only, mmap'd) plus the
//! in-memory variant graph, both built once at startup.

use std::sync::Arc;

use r2d2_sqlite::SqliteConnectionManager;

use crate::graph::VariantGraph;

pub type Pool = r2d2::Pool<SqliteConnectionManager>;

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    pub graph: Arc<VariantGraph>,
    /// shared HTTP client for the handwriting proxy (/recognize)
    pub http: reqwest::Client,
}

impl AppState {
    pub fn load(db_path: &str) -> anyhow::Result<Self> {
        // Read-only, with generous mmap so the OS page cache serves hot pages.
        let manager = SqliteConnectionManager::file(db_path).with_flags(
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ).with_init(|c| {
            c.execute_batch(
                "PRAGMA query_only = ON; \
                 PRAGMA mmap_size = 268435456; \
                 PRAGMA cache_size = -32000; \
                 PRAGMA temp_store = MEMORY;",
            )
        });
        let pool = r2d2::Pool::builder().max_size(8).build(manager)?;

        let graph = {
            let conn = pool.get()?;
            VariantGraph::load(&conn)?
        };
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .user_agent("kanzi/0.0 (+https://miro.build)")
            .build()?;
        Ok(AppState { pool, graph: Arc::new(graph), http })
    }
}
