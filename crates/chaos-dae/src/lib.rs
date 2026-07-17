//! chaos-dae — resolve and manage the vendored `dae` binary.

use std::path::{Path, PathBuf};

/// Resolve the path to the `dae` binary.
///
/// Order:
/// 1. `CHAOS_DAE_BIN` environment variable (if non-empty)
/// 2. `third_party/dae/current/dae` relative to the process cwd (if present)
/// 3. `None`
pub fn resolve_dae_bin() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CHAOS_DAE_BIN") {
        let path = path.trim();
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }

    let candidate = Path::new("third_party/dae/current/dae");
    if candidate.is_file() {
        return Some(candidate.to_path_buf());
    }

    None
}

/// Whether `path` looks like a usable dae binary (exists and is a regular file).
pub fn dae_bin_ok(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    // Serialize env-mutating tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_prefers_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("CHAOS_DAE_BIN", "/tmp/custom-dae");
        let got = resolve_dae_bin();
        std::env::remove_var("CHAOS_DAE_BIN");
        assert_eq!(got, Some(PathBuf::from("/tmp/custom-dae")));
    }

    #[test]
    fn resolve_falls_back_to_vendored_when_present() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("CHAOS_DAE_BIN");

        let dir = Path::new("third_party/dae/current");
        let file = dir.join("dae");
        let _ = fs::remove_file(&file);
        let created = fs::create_dir_all(dir).is_ok() && fs::write(&file, b"fake").is_ok();
        if !created {
            return;
        }

        let got = resolve_dae_bin();
        let _ = fs::remove_file(&file);
        assert_eq!(got, Some(PathBuf::from("third_party/dae/current/dae")));
    }

    #[test]
    fn dae_bin_ok_false_for_missing() {
        assert!(!dae_bin_ok(Path::new("/no/such/dae-binary-xyz")));
    }
}
