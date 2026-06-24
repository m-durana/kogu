//! Shared application state: a small SQLite connection pool (read-only, mmap'd) plus the
//! in-memory variant graph, both built once at startup.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use r2d2_sqlite::SqliteConnectionManager;

use crate::graph::VariantGraph;

pub type Pool = r2d2::Pool<SqliteConnectionManager>;

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    pub graph: Arc<VariantGraph>,
    /// shared HTTP client for the handwriting proxy (/recognize) and the /mt translate proxy
    pub http: reqwest::Client,
    /// OCR engine (PaddleOCR via ONNX); None if models are unavailable
    pub ocr: Option<Arc<oar_ocr::prelude::OAROCR>>,
    /// in-memory cache for the /mt translate proxy ("sl|q" → (translation, detected_source)); the DB
    /// pool is read-only so we can't persist there. Lost on restart, which is fine — cheap to refetch.
    pub mt_cache: Arc<Mutex<HashMap<String, (String, String)>>>,
    /// in-memory cache of proxied pronunciation clips ("zh/ni3" → mp3 bytes). The zh/yue clips are
    /// fetched from upstream CDNs server-side and served same-origin, so they work where those CDNs are
    /// blocked (mainland China) or unreachable from the device; immutable, so caching is safe.
    pub clip_cache: Arc<Mutex<HashMap<String, Vec<u8>>>>,
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
            .user_agent("kogu/0.0 (+https://miro.build)")
            .build()?;
        let ocr = crate::ocr::load_engine().map(Arc::new);
        let mt_cache = Arc::new(Mutex::new(HashMap::new()));
        let clip_cache = Arc::new(Mutex::new(HashMap::new()));
        Ok(AppState { pool, graph: Arc::new(graph), http, ocr, mt_cache, clip_cache })
    }
}
