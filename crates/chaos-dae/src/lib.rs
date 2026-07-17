//! chaos-dae — resolve and manage the vendored `dae` binary.
//!
//! # Runtime control (MVP)
//!
//! Config is written to `{work_dir}/config.dae`. Lifecycle uses a pid file at
//! `{work_dir}/dae.pid` and the CLI:
//!
//! - start / restart: `{bin} run -c {work_dir}` (detached)
//! - reload: if a live pid exists, terminate it, then start again
//! - stop: `kill` the pid and remove `dae.pid`
//!
//! Flags match a typical dae CLI surface (`run -c <config-dir-or-file>`). Real
//! eBPF apply is out of scope for unit tests; use `tests/fixtures/fake-dae.sh`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

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

/// Manages a local dae process bound to a work directory.
#[derive(Debug, Clone)]
pub struct DaeManager {
    pub bin: PathBuf,
    pub work_dir: PathBuf,
}

impl DaeManager {
    pub fn new(bin: impl Into<PathBuf>, work_dir: impl Into<PathBuf>) -> Self {
        Self {
            bin: bin.into(),
            work_dir: work_dir.into(),
        }
    }

    pub fn config_path(&self) -> PathBuf {
        self.work_dir.join("config.dae")
    }

    pub fn pid_path(&self) -> PathBuf {
        self.work_dir.join("dae.pid")
    }

    /// Write `content` to `{work_dir}/config.dae`, creating `work_dir` if needed.
    pub async fn write_config(&self, content: &str) -> Result<PathBuf> {
        tokio::fs::create_dir_all(&self.work_dir)
            .await
            .with_context(|| format!("create work_dir {}", self.work_dir.display()))?;
        let path = self.config_path();
        tokio::fs::write(&path, content)
            .await
            .with_context(|| format!("write config {}", path.display()))?;
        Ok(path)
    }

    /// Whether the process recorded in `dae.pid` is still alive.
    pub fn is_running(&self) -> bool {
        match self.read_pid() {
            Some(pid) => process_alive(pid),
            None => false,
        }
    }

    /// Stop a running dae process (if any) and clear the pid file.
    pub async fn stop(&self) -> Result<()> {
        if let Some(pid) = self.read_pid() {
            if process_alive(pid) {
                let status = Command::new("kill")
                    .arg(pid.to_string())
                    .status()
                    .with_context(|| format!("kill dae pid {pid}"))?;
                if !status.success() && process_alive(pid) {
                    // Escalate once if still alive.
                    let _ = Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .status();
                }
            }
            let _ = tokio::fs::remove_file(self.pid_path()).await;
        }
        Ok(())
    }

    /// Ensure dae is running with the current config.
    ///
    /// MVP strategy: if a live pid exists, kill it; then spawn
    /// `{bin} run -c {work_dir}` detached and write `{work_dir}/dae.pid`.
    pub async fn reload(&self) -> Result<()> {
        if !self.bin.is_file() {
            bail!("dae binary missing or not a file: {}", self.bin.display());
        }

        tokio::fs::create_dir_all(&self.work_dir)
            .await
            .with_context(|| format!("create work_dir {}", self.work_dir.display()))?;

        if self.is_running() {
            self.stop().await?;
        } else {
            // Stale pid file.
            let _ = tokio::fs::remove_file(self.pid_path()).await;
        }

        self.spawn_run().await
    }

    async fn spawn_run(&self) -> Result<()> {
        let mut child = Command::new(&self.bin)
            .arg("run")
            .arg("-c")
            .arg(&self.work_dir)
            .current_dir(&self.work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| {
                format!(
                    "spawn `{} run -c {}`",
                    self.bin.display(),
                    self.work_dir.display()
                )
            })?;

        let pid = child.id();
        // Detach: do not wait; dropping Child without wait leaves the process running
        // (or exiting on its own for short-lived fixtures).
        std::mem::forget(child);

        tokio::fs::write(self.pid_path(), pid.to_string())
            .await
            .with_context(|| format!("write pid file {}", self.pid_path().display()))?;
        Ok(())
    }

    fn read_pid(&self) -> Option<u32> {
        let raw = std::fs::read_to_string(self.pid_path()).ok()?;
        raw.trim().parse().ok()
    }
}

fn process_alive(pid: u32) -> bool {
    // `kill -0` checks existence / permission without sending a signal.
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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
