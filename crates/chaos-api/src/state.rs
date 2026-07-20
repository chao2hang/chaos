//! Shared application state.

use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub jwt_secret: Arc<String>,
    /// Path to the `chaos-prober` binary for real proxy latency tests.
    /// `None` = prober unavailable, fall back to TCP connect probes.
    pub prober_bin: Option<PathBuf>,
    /// Serialize apply/stop/publish lifecycle operations. The runtime is a
    /// single process with one config and pid file, so concurrent requests are
    /// not independent operations.
    pub runtime_lock: Arc<Mutex<()>>,
}

impl AppState {
    pub fn new(pool: SqlitePool, jwt_secret: String) -> Self {
        Self {
            pool,
            jwt_secret: Arc::new(jwt_secret),
            prober_bin: resolve_prober_bin(),
            runtime_lock: Arc::new(Mutex::new(())),
        }
    }
}

/// Resolve the path to the `chaos-prober` binary.
///
/// Order:
/// 1. `CHAOS_PROBER_BIN` environment variable
/// 2. `third_party/chaos-prober` relative to cwd
/// 3. `None` (fall back to TCP probes)
fn resolve_prober_bin() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CHAOS_PROBER_BIN") {
        let path = path.trim();
        if !path.is_empty() {
            let p = PathBuf::from(path);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let candidate = PathBuf::from("third_party/chaos-prober");
    if candidate.is_file() {
        return Some(candidate);
    }
    None
}
