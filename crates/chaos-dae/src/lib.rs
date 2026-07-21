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
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};

/// Backend-neutral data-plane capability exposed to the control plane.
///
/// Linux currently uses dae. Windows deliberately reports an unavailable
/// backend until a signed Wintun + sing-box/mihomo implementation is wired in;
/// this prevents the API from accidentally trying to run a Linux dae config on
/// Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataPlaneStatus {
    pub kind: &'static str,
    pub ready: bool,
    pub reason: &'static str,
}

pub trait DataPlaneBackend: Send + Sync {
    fn status(&self) -> DataPlaneStatus;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LinuxDaeBackend;

#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsWintunBackend;

impl DataPlaneBackend for LinuxDaeBackend {
    fn status(&self) -> DataPlaneStatus {
        let ready = resolve_dae_bin().is_some_and(|path| dae_bin_ok(&path));
        DataPlaneStatus {
            kind: "linux-dae",
            ready,
            reason: if ready {
                "dae backend ready"
            } else {
                "dae binary missing"
            },
        }
    }
}

impl DataPlaneBackend for WindowsWintunBackend {
    fn status(&self) -> DataPlaneStatus {
        DataPlaneStatus {
            kind: "windows-wintun-engine",
            ready: false,
            reason: "Windows data plane requires a signed Wintun adapter and a configured sing-box or mihomo engine",
        }
    }
}

pub enum PlatformBackend {
    Linux(LinuxDaeBackend),
    Windows(WindowsWintunBackend),
}

impl DataPlaneBackend for PlatformBackend {
    fn status(&self) -> DataPlaneStatus {
        match self {
            Self::Linux(backend) => backend.status(),
            Self::Windows(backend) => backend.status(),
        }
    }
}

impl PlatformBackend {
    pub fn status(&self) -> DataPlaneStatus {
        <Self as DataPlaneBackend>::status(self)
    }
}

pub fn platform_backend() -> PlatformBackend {
    #[cfg(windows)]
    {
        return PlatformBackend::Windows(WindowsWintunBackend);
    }
    #[cfg(not(windows))]
    {
        PlatformBackend::Linux(LinuxDaeBackend)
    }
}

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

/// Result of a hot-reload attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadOutcome {
    /// Config reloaded without interrupting connections.
    Hot,
    /// dae was not running; cold-started instead.
    ColdStart,
    /// Hot reload failed; fell back to kill+restart.
    ColdFallback,
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

    /// dae searches the config directory for GeoIP data before global paths.
    pub fn geoip_path(&self) -> PathBuf {
        self.work_dir.join("geoip.dat")
    }

    /// Atomically replace the GeoIP dataset used by dae routing rules.
    pub async fn write_geoip_data(&self, content: &[u8]) -> Result<PathBuf> {
        if content.len() < 1024 {
            bail!("geoip dataset is unexpectedly small");
        }
        tokio::fs::create_dir_all(&self.work_dir)
            .await
            .with_context(|| format!("create work_dir {}", self.work_dir.display()))?;
        secure_work_dir(&self.work_dir)?;
        let path = self.geoip_path();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let temp = self.work_dir.join(format!("geoip.dat.tmp-{nonce}"));
        tokio::fs::write(&temp, content)
            .await
            .with_context(|| format!("write geoip data {}", temp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o644))
                .with_context(|| format!("chmod 0644 {}", temp.display()))?;
        }
        #[cfg(windows)]
        if path.exists() {
            let _ = tokio::fs::remove_file(&path).await;
        }
        tokio::fs::rename(&temp, &path)
            .await
            .with_context(|| format!("replace geoip data {}", path.display()))?;
        Ok(path)
    }

    /// Write `content` to `{work_dir}/config.dae`, creating `work_dir` if needed.
    ///
    /// Mode is forced to `0600`: dae rejects configs that are group/world
    /// readable or writable (e.g. default umask `0644`).
    pub async fn write_config(&self, content: &str) -> Result<PathBuf> {
        tokio::fs::create_dir_all(&self.work_dir)
            .await
            .with_context(|| format!("create work_dir {}", self.work_dir.display()))?;
        secure_work_dir(&self.work_dir)?;
        let path = self.config_path();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let temp = self.work_dir.join(format!("config.dae.tmp-{nonce}"));
        tokio::fs::write(&temp, content)
            .await
            .with_context(|| format!("write config {}", temp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&temp, perms)
                .with_context(|| format!("chmod 0600 {}", temp.display()))?;
        }
        #[cfg(windows)]
        if path.exists() {
            // Windows cannot rename over an existing file. The destination is
            // still private to the work directory; the next write is complete
            // before this replacement occurs.
            let _ = tokio::fs::remove_file(&path).await;
        }
        tokio::fs::rename(&temp, &path)
            .await
            .with_context(|| format!("replace config {}", path.display()))?;
        Ok(path)
    }

    /// Whether the process recorded in `dae.pid` is still alive.
    pub fn is_running(&self) -> bool {
        match self.read_pid() {
            Some(pid) => process_alive(pid, Some(&self.bin), Some(&self.config_path())),
            None => false,
        }
    }

    /// Ask dae to parse the staged config before interrupting a running process.
    pub async fn validate_config(&self) -> Result<()> {
        if !self.bin.is_file() {
            bail!("dae binary missing or not a file: {}", self.bin.display());
        }
        let bin = std::fs::canonicalize(&self.bin)
            .with_context(|| format!("canonicalize dae bin {}", self.bin.display()))?;
        let config = std::fs::canonicalize(self.config_path())
            .with_context(|| format!("canonicalize config {}", self.config_path().display()))?;
        let output = tokio::process::Command::new(&bin)
            .arg("validate")
            .arg("-c")
            .arg(&config)
            .output()
            .await
            .with_context(|| format!("run `{} validate -c {}`", bin.display(), config.display()))?;
        if !output.status.success() {
            let detail = if output.stderr.is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            } else {
                String::from_utf8_lossy(&output.stderr).trim().to_string()
            };
            bail!(
                "dae config validation failed ({}): {}",
                output.status,
                if detail.is_empty() {
                    "no diagnostic"
                } else {
                    &detail
                }
            );
        }
        Ok(())
    }

    /// Stop a running dae process (if any) and clear the pid file.
    pub async fn stop(&self) -> Result<()> {
        if let Some(pid) = self.read_pid() {
            if process_alive(pid, Some(&self.bin), Some(&self.config_path())) {
                terminate_process(pid).with_context(|| format!("stop dae pid {pid}"))?;
                for _ in 0..40 {
                    if !process_alive(pid, Some(&self.bin), Some(&self.config_path())) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                if process_alive(pid, Some(&self.bin), Some(&self.config_path())) {
                    force_kill_process(pid).with_context(|| format!("force stop dae pid {pid}"))?;
                    for _ in 0..20 {
                        if !process_alive(pid, Some(&self.bin), Some(&self.config_path())) {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                }
                if process_alive(pid, Some(&self.bin), Some(&self.config_path())) {
                    bail!("dae pid {pid} did not exit after stop");
                }
            }
            let _ = tokio::fs::remove_file(self.pid_path()).await;
        }
        Ok(())
    }

    /// Attempt a zero-downtime config reload via `dae reload <pid>`.
    ///
    /// If dae is running, sends SIGUSR1 through the dae CLI reload protocol.
    /// If dae is not running, falls back to a cold start.
    /// Returns the outcome so callers can report the method used.
    pub async fn hot_reload(&self) -> Result<ReloadOutcome> {
        if !self.bin.is_file() {
            bail!("dae binary missing or not a file: {}", self.bin.display());
        }
        let config = self.config_path();
        if !config.is_file() {
            bail!("config file missing: {}", config.display());
        }

        // If dae is not running, cold-start it.
        if !self.is_running() {
            tokio::fs::create_dir_all(&self.work_dir)
                .await
                .with_context(|| format!("create work_dir {}", self.work_dir.display()))?;
            secure_work_dir(&self.work_dir)?;
            let _ = tokio::fs::remove_file(self.pid_path()).await;
            self.spawn_run().await?;
            return Ok(ReloadOutcome::ColdStart);
        }

        let pid = match self.read_pid() {
            Some(pid) => pid,
            None => {
                // Stale state: no pid but is_running() was true (shouldn't happen).
                self.reload().await?;
                return Ok(ReloadOutcome::ColdFallback);
            }
        };

        let bin = std::fs::canonicalize(&self.bin)
            .with_context(|| format!("canonicalize dae bin {}", self.bin.display()))?;

        // Execute `dae reload <pid>` which handles the SIGUSR1 + progress file protocol.
        let output = tokio::process::Command::new(&bin)
            .arg("reload")
            .arg(pid.to_string())
            .output()
            .await
            .with_context(|| format!("run `{} reload {}`", bin.display(), pid))?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        if output.status.success() {
            tracing::info!(pid, "dae hot reload succeeded");
            return Ok(ReloadOutcome::Hot);
        }

        // Reload failed — check if it's a "busy" condition or a hard error.
        let detail = if stderr.is_empty() { &stdout } else { &stderr };
        tracing::warn!(
            pid,
            status = %output.status,
            detail,
            "dae hot reload failed; falling back to cold restart"
        );

        // Fallback: kill + restart.
        self.reload().await.with_context(|| {
            format!("cold restart after hot reload failure: {detail}")
        })?;
        Ok(ReloadOutcome::ColdFallback)
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
        secure_work_dir(&self.work_dir)?;

        if self.is_running() {
            self.stop().await?;
        } else {
            // Stale pid file.
            let _ = tokio::fs::remove_file(self.pid_path()).await;
        }

        self.spawn_run().await
    }

    async fn spawn_run(&self) -> Result<()> {
        let config = self.config_path();
        if !config.is_file() {
            bail!("config file missing: {}", config.display());
        }

        // Absolute paths: we `current_dir` into work_dir, so a relative bin like
        // `third_party/dae/current/dae` would otherwise fail with ENOENT.
        let bin = std::fs::canonicalize(&self.bin)
            .with_context(|| format!("canonicalize dae bin {}", self.bin.display()))?;
        let config = std::fs::canonicalize(&config)
            .with_context(|| format!("canonicalize config {}", config.display()))?;
        let work_dir = std::fs::canonicalize(&self.work_dir)
            .with_context(|| format!("canonicalize work_dir {}", self.work_dir.display()))?;

        // Re-assert mode in case an older write left 0644 on disk.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600));
        }

        let log_path = work_dir.join("dae.log");
        let log_file = std::fs::File::create(&log_path)
            .with_context(|| format!("create log {}", log_path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&log_path, std::fs::Permissions::from_mode(0o600))
                .with_context(|| format!("chmod 0600 {}", log_path.display()))?;
        }
        let log_err = log_file
            .try_clone()
            .with_context(|| format!("clone log handle {}", log_path.display()))?;

        // dae manages its own pidfile under /var/run by default; we track work_dir/dae.pid.
        // Default: --disable-sudo so the API never hangs on a password prompt.
        // Set CHAOS_DAE_ALLOW_SUDO=1 if passwordless sudo is configured for dae.
        let allow_sudo = std::env::var("CHAOS_DAE_ALLOW_SUDO")
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let mut cmd = Command::new(&bin);
        cmd.arg("run")
            .arg("-c")
            .arg(&config)
            .arg("--disable-pidfile");
        if !allow_sudo {
            cmd.arg("--disable-sudo");
        }
        let mut child = cmd
            .current_dir(&work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_err))
            .spawn()
            .with_context(|| format!("spawn `{} run -c {}`", bin.display(), config.display()))?;

        let pid = child.id();
        tokio::fs::write(self.pid_path(), pid.to_string())
            .await
            .with_context(|| format!("write pid file {}", self.pid_path().display()))?;

        // Grace period: real dae may exit immediately on bad config/permissions.
        // Slightly longer so permission/config fatals flush to dae.log.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Reap if already exited (avoids zombie making kill -0 look "alive").
        let early_exit = match child.try_wait() {
            Ok(Some(status)) => Some(status),
            Ok(None) => None,
            Err(_) => None,
        };

        if early_exit.is_some() || !process_alive(pid, Some(&self.bin), Some(&self.config_path())) {
            let log_tail = tokio::fs::read_to_string(&log_path)
                .await
                .unwrap_or_default();
            let _ = tokio::fs::remove_file(self.pid_path()).await;
            let excerpt: String = log_tail
                .chars()
                .rev()
                .take(2000)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            bail!(
                "dae exited immediately after start; log excerpt:\n{}",
                if excerpt.trim().is_empty() {
                    "(empty log)".to_string()
                } else {
                    excerpt
                }
            );
        }

        // Detach: leave process running without reaping here.
        std::mem::forget(child);
        Ok(())
    }

    fn read_pid(&self) -> Option<u32> {
        let raw = std::fs::read_to_string(self.pid_path()).ok()?;
        raw.trim().parse().ok()
    }
}

#[cfg(unix)]
fn process_alive(pid: u32, expected_bin: Option<&Path>, expected_config: Option<&Path>) -> bool {
    // `kill -0` checks existence / permission without sending a signal.
    let alive = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !alive {
        return false;
    }
    use std::os::unix::ffi::OsStrExt;
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    if let Some(expected_bin) = expected_bin {
        let expected = absolute_path(expected_bin);
        let deleted = PathBuf::from(format!("{} (deleted)", expected.display()));
        let exe_matches = std::fs::read_link(format!("/proc/{pid}/exe"))
            .ok()
            .is_some_and(|path| {
                path == expected
                    || path == deleted
                    || std::fs::canonicalize(path).is_ok_and(|path| path == expected)
            });
        let expected_bytes = expected.as_os_str().as_bytes();
        let command_matches = !expected_bytes.is_empty()
            && cmdline
                .windows(expected_bytes.len())
                .any(|window| window == expected_bytes);
        if exe_matches || command_matches {
            return true;
        }
        if expected_bin.is_file() {
            return false;
        }
    }

    // If the binary was deleted or its configured path was lost, the private
    // pid file is not enough by itself: require the daemon's exact config path
    // and CLI shape before signalling the process.
    let Some(config) = expected_config.map(absolute_path) else {
        return false;
    };
    let args: Vec<&[u8]> = cmdline
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .collect();
    args.contains(&b"run".as_slice())
        && args.contains(&b"-c".as_slice())
        && args.contains(&config.as_os_str().as_bytes())
}

fn absolute_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    })
}

fn secure_work_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("chmod work directory {}", path.display()))?;
    }
    Ok(())
}

#[cfg(windows)]
fn process_alive(pid: u32, _expected_bin: Option<&Path>, _expected_config: Option<&Path>) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

#[cfg(unix)]
fn terminate_process(pid: u32) -> Result<()> {
    let status = Command::new("kill").arg(pid.to_string()).status()?;
    if status.success() {
        Ok(())
    } else {
        bail!("kill returned {status}")
    }
}

#[cfg(windows)]
fn terminate_process(pid: u32) -> Result<()> {
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string()])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        bail!("taskkill returned {status}")
    }
}

#[cfg(unix)]
fn force_kill_process(pid: u32) -> Result<()> {
    let status = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        bail!("kill -9 returned {status}")
    }
}

#[cfg(windows)]
fn force_kill_process(pid: u32) -> Result<()> {
    let status = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        bail!("taskkill /F returned {status}")
    }
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

    #[test]
    fn platform_backend_reports_an_explicit_kind() {
        let status = platform_backend().status();
        #[cfg(windows)]
        {
            assert_eq!(status.kind, "windows-wintun-engine");
            assert!(!status.ready);
        }
        #[cfg(not(windows))]
        assert_eq!(status.kind, "linux-dae");
    }
}
