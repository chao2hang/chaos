//! Shared application state.

use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
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
    /// Serialize self-update operations.
    pub update_lock: Arc<Mutex<()>>,
}

impl AppState {
    pub fn new(pool: SqlitePool, jwt_secret: String) -> Self {
        Self {
            pool,
            jwt_secret: Arc::new(jwt_secret),
            prober_bin: resolve_prober_bin(std::env::current_exe().ok()),
            runtime_lock: Arc::new(Mutex::new(())),
            update_lock: Arc::new(Mutex::new(())),
        }
    }
}

/// Resolve the path to the `chaos-prober` binary.
///
/// Order:
/// 1. `CHAOS_PROBER_BIN` environment variable
/// 2. `chaos-prober` next to the running `chaos-api` executable (installed package layout)
/// 3. `/usr/lib/chaos/bin/chaos-prober`
/// 4. `third_party/chaos-prober` relative to cwd (development layout)
fn resolve_prober_bin(current_exe: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CHAOS_PROBER_BIN") {
        let path = path.to_string_lossy();
        let path = path.trim();
        if !path.is_empty() {
            let p = PathBuf::from(path);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    let candidates = [
        current_exe
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .map(|dir| dir.join("chaos-prober")),
        Some(PathBuf::from("/usr/lib/chaos/bin/chaos-prober")),
        Some(PathBuf::from("third_party/chaos-prober")),
    ];

    candidates.into_iter().flatten().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex as StdMutex;

    static ENV_LOCK: StdMutex<()> = StdMutex::new(());

    #[test]
    fn resolves_prober_next_to_api_binary() {
        let dir = std::env::temp_dir().join(format!("chaos-prober-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let prober = dir.join("chaos-prober");
        fs::write(&prober, b"").unwrap();

        let resolved = resolve_prober_bin(Some(dir.join("chaos-api")));
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(resolved.as_deref(), Some(prober.as_path()));
    }

    #[test]
    fn env_override_takes_precedence() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir =
            std::env::temp_dir().join(format!("chaos-prober-env-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let sibling = dir.join("chaos-prober");
        let override_path = dir.join("custom-prober");
        fs::write(&sibling, b"sibling").unwrap();
        fs::write(&override_path, b"override").unwrap();

        // SAFETY: tests are serialized by ENV_LOCK while mutating process-global env.
        unsafe { std::env::set_var("CHAOS_PROBER_BIN", &override_path) };
        let resolved = resolve_prober_bin(Some(dir.join("chaos-api")));
        unsafe { std::env::remove_var("CHAOS_PROBER_BIN") };

        let _ = fs::remove_dir_all(&dir);
        assert_eq!(resolved.as_deref(), Some(override_path.as_path()));
    }
}
