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
//!
//! # Platform
//!
//! The vendored `dae` data plane is Linux-only, and so is chaos: the manager
//! relies on `/proc`, POSIX signals, `getifaddrs(3)` and netns mounts. There is
//! no Windows backend — a non-Unix build fails early with a `compile_error!`
//! below instead of failing at runtime.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};

#[cfg(not(unix))]
compile_error!(
    "chaos is Unix-only: the data plane shells out to `dae`, which needs a Linux \
     kernel with eBPF support. The Windows backend was removed in 0.1.27."
);

/// Reported to clients as `data_plane`; the API and the web console match on
/// this value, so treat it as part of the HTTP contract.
pub const DATA_PLANE_KIND: &str = "linux-dae";

/// Capability report for the dae data plane, surfaced through `/health` and the
/// runtime endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataPlaneStatus {
    pub kind: &'static str,
    pub ready: bool,
    pub reason: &'static str,
}

/// Whether the vendored `dae` binary is present and usable.
pub fn data_plane_status() -> DataPlaneStatus {
    let ready = resolve_dae_bin().is_some_and(|path| dae_bin_ok(&path));
    DataPlaneStatus {
        kind: DATA_PLANE_KIND,
        ready,
        reason: if ready {
            "dae backend ready"
        } else {
            "dae binary missing"
        },
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

    /// dae searches the config directory for Geosite data before global paths.
    pub fn geosite_path(&self) -> PathBuf {
        self.work_dir.join("geosite.dat")
    }

    /// Atomically replace the GeoIP dataset used by dae routing rules.
    pub async fn write_geoip_data(&self, content: &[u8]) -> Result<PathBuf> {
        self.write_geo_dataset("geoip.dat", content).await
    }

    /// Atomically replace the Geosite dataset used by dae routing rules.
    pub async fn write_geosite_data(&self, content: &[u8]) -> Result<PathBuf> {
        self.write_geo_dataset("geosite.dat", content).await
    }

    async fn write_geo_dataset(&self, filename: &str, content: &[u8]) -> Result<PathBuf> {
        if content.len() < 1024 {
            bail!("{filename} dataset is unexpectedly small");
        }
        tokio::fs::create_dir_all(&self.work_dir)
            .await
            .with_context(|| format!("create work_dir {}", self.work_dir.display()))?;
        secure_work_dir(&self.work_dir)?;
        let path = self.work_dir.join(filename);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let temp = self.work_dir.join(format!("{filename}.tmp-{nonce}"));
        tokio::fs::write(&temp, content)
            .await
            .with_context(|| format!("write geo data {}", temp.display()))?;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o644))
            .with_context(|| format!("chmod 0644 {}", temp.display()))?;
        tokio::fs::rename(&temp, &path)
            .await
            .with_context(|| format!("replace geo data {}", path.display()))?;
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
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&temp, perms)
            .with_context(|| format!("chmod 0600 {}", temp.display()))?;
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
        self.reload()
            .await
            .with_context(|| format!("cold restart after hot reload failure: {detail}"))?;
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
        let _ = std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600));

        let log_path = work_dir.join("dae.log");
        // Append rather than truncate: a restart used to wipe the log, which is
        // exactly the history needed to explain why the restart happened.
        rotate_log_if_needed(&log_path)?;
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("open log {}", log_path.display()))?;
        std::fs::set_permissions(&log_path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("chmod 0600 {}", log_path.display()))?;
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

        // dae sets up its own netns (mounted at /run/netns/daens) at startup and
        // bails with "file exists" if a stale mount from an unclean exit is still
        // around. Safe here: all paths into spawn_run have dae stopped already.
        cleanup_stale_netns();

        // Compatibility defaults for virtualized / vNIC environments (KVM/virtio):
        // dae's userspace TCP relay combined with NIC checksum/segmentation
        // offloads can corrupt forwarded payloads (TLS "bad record mac",
        // resets, empty bodies). Disabling the eBPF TCP-relay offload and
        // quic-go GSO avoids those paths. Operators can opt out by setting the
        // vars explicitly (e.g. CHAOS_DAE_DISABLE_TCP_RELAY_OFFLOAD=0).
        apply_compat_env(&mut cmd, "DAE_DISABLE_TCP_RELAY_OFFLOAD");
        apply_compat_env(&mut cmd, "QUIC_GO_DISABLE_GSO");
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
            let excerpt = read_log_excerpt(&log_path, 2000);
            let _ = tokio::fs::remove_file(self.pid_path()).await;
            bail!(
                "dae exited immediately after start; log excerpt:\n{}",
                if excerpt.trim().is_empty() {
                    "(empty log)".to_string()
                } else {
                    excerpt
                }
            );
        }

        // Detach but keep reaping: a background thread owns the Child handle and
        // `wait()`s on exit, so a dae that dies later (crash, OOM, kill -9) is
        // promptly reaped instead of lingering as a zombie under chaos-api.
        std::thread::spawn(move || match child.wait() {
            Ok(status) => {
                tracing::info!(pid, status = %status, "dae process exited (reaped)");
            }
            Err(error) => {
                tracing::warn!(pid, error = %error, "failed to reap dae process");
            }
        });
        Ok(())
    }

    fn read_pid(&self) -> Option<u32> {
        let raw = std::fs::read_to_string(self.pid_path()).ok()?;
        raw.trim().parse().ok()
    }
}

fn process_alive(pid: u32, expected_bin: Option<&Path>, expected_config: Option<&Path>) -> bool {
    // Signal 0 checks existence / permission without sending a signal. Go
    // through the syscall rather than the `kill(1)` binary: spawning a process
    // per probe costs a fork, and every "process is gone" answer (the normal
    // case for a stale pid file) printed `kill: (PID): No such process` into the
    // service log.
    let alive = match signal_process(pid, 0) {
        Ok(()) => true,
        // EPERM means the process exists but belongs to another user.
        Err(error) => error.raw_os_error() == Some(libc::EPERM),
    };
    if !alive {
        return false;
    }
    // A zombie answers `kill -0` but is dead. Treat it as not alive so that
    // is_running()/stop() never stall on an unreaped child (the spawn_run
    // reaper thread reaps promptly, but between exit and reap this can occur).
    if proc_stat_state(pid) == Some('Z') {
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

/// Parse the process state field (3rd field) from `/proc/<pid>/stat`.
///
/// The comm field is wrapped in parens and may itself contain spaces or parens
/// (e.g. `(dae (worker))`), so we scan for the *last* `)` rather than splitting
/// on whitespace. Returns `None` for malformed input.
fn parse_proc_stat_state(stat: &str) -> Option<char> {
    let close = stat.rfind(')')?;
    stat.get(close + 1..)?.trim_start().chars().next()
}

/// Process state for `pid` per `/proc/<pid>/stat`, if readable.
fn proc_stat_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_proc_stat_state(&stat)
}

/// Remove a stale dae network-namespace mount left by an unclean exit (host
/// reboot mid-upgrade, OOM, `kill -9`). dae refuses to start when
/// `/run/netns/daens` already exists. Only invoked when no dae process is
/// running (every call path into `spawn_run` guarantees that), so this is safe;
/// failures are tolerated and logged at debug level.
fn cleanup_stale_netns() {
    const NETNS_PATH: &str = "/run/netns/daens";

    // Only unmount when the path really is a mount point: `umount` on an
    // unmounted path exits non-zero and prints `no mount point specified`,
    // which used to land in the service log on every single start.
    if is_mount_point(NETNS_PATH) {
        run_quiet("umount", &[NETNS_PATH]);
        tracing::debug!(path = NETNS_PATH, "removed stale dae netns mount");
    }
    run_quiet("rm", &["-f", NETNS_PATH]);
}

/// Whether `/proc/self/mountinfo` lists `path` as a mount point (field 5).
fn is_mount_point(path: &str) -> bool {
    std::fs::read_to_string("/proc/self/mountinfo")
        .map(|info| {
            info.lines()
                .any(|line| line.split_whitespace().nth(4) == Some(path))
        })
        .unwrap_or(false)
}

/// Run a best-effort helper command with its output discarded.
fn run_quiet(cmd: &str, args: &[&str]) {
    match Command::new(cmd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => tracing::debug!(cmd, %status, "helper command did not succeed"),
        Err(error) => tracing::debug!(cmd, error = %error, "helper command unavailable"),
    }
}

/// Set a dae compatibility env var to `"1"` on `cmd` unless the operator has
/// already provided an explicit value.
///
/// The presence of `CHAOS_<NAME>` in the control-plane environment also counts
/// as an explicit override, so deployments can e.g. set
/// `CHAOS_DAE_DISABLE_TCP_RELAY_OFFLOAD=0` to force the feature back on.
/// Decide the value to set for a dae compatibility env var.
///
/// Returns `None` when the var is already present in the environment and should
/// be inherited untouched. Otherwise returns `Some(value)`: `"1"` by default,
/// or the value of `CHAOS_<NAME>` if the operator set an explicit override.
fn compat_env_value(name: &str) -> Option<String> {
    if std::env::var_os(name).is_some() {
        return None;
    }
    match std::env::var(format!("CHAOS_{name}")) {
        Ok(value) => Some(value),
        Err(_) => Some("1".to_string()),
    }
}

fn apply_compat_env(cmd: &mut Command, name: &str) {
    if let Some(value) = compat_env_value(name) {
        cmd.env(name, value);
    }
}

/// Size at which `dae.log` is rotated to `dae.log.1`.
///
/// The data plane writes one line per proxied connection, which reaches tens of
/// megabytes a day on a busy host. Without a bound the file grows until the
/// disk fills; with it, the work directory holds at most two generations.
const DEFAULT_LOG_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Rotation threshold in bytes, overridable with `CHAOS_DAE_LOG_MAX_BYTES`.
fn log_max_bytes() -> u64 {
    std::env::var("CHAOS_DAE_LOG_MAX_BYTES")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|bytes| *bytes >= 1024)
        .unwrap_or(DEFAULT_LOG_MAX_BYTES)
}

/// Rotate `dae.log` to `dae.log.1` once it passes [`log_max_bytes`].
///
/// The log is opened in append mode, so this is what keeps it bounded. Rotation
/// runs from `spawn_run`, which every caller reaches only after stopping dae, so
/// no process holds the file open and a plain rename cannot lose writes.
fn rotate_log_if_needed(log_path: &Path) -> Result<()> {
    let Ok(metadata) = std::fs::metadata(log_path) else {
        return Ok(()); // No log yet.
    };
    if metadata.len() < log_max_bytes() {
        return Ok(());
    }

    let rotated = match log_path.file_name().and_then(|name| name.to_str()) {
        Some(name) => log_path.with_file_name(format!("{name}.1")),
        None => log_path.with_extension("1"),
    };
    std::fs::rename(log_path, &rotated).with_context(|| {
        format!(
            "rotate dae log {} -> {}",
            log_path.display(),
            rotated.display()
        )
    })?;
    tracing::info!(
        bytes = metadata.len(),
        path = %rotated.display(),
        "rotated dae log"
    );
    Ok(())
}

/// Trailing `max_chars` characters of the log.
///
/// Only the end of the file is read: this runs on the startup-failure path,
/// where loading a multi-megabyte log to show a few hundred characters would be
/// wasteful.
fn read_log_excerpt(log_path: &Path, max_chars: usize) -> String {
    use std::io::{Read, Seek};

    const READ_BYTES: u64 = 16 * 1024;

    let Ok(mut file) = std::fs::File::open(log_path) else {
        return String::new();
    };
    let len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let start = len.saturating_sub(READ_BYTES);
    if start > 0 && file.seek(std::io::SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    // A window cut at an arbitrary offset can split a UTF-8 sequence, so decode
    // lossily rather than failing.
    let text = String::from_utf8_lossy(&buf);
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        text.into_owned()
    } else {
        chars[chars.len() - max_chars..].iter().collect()
    }
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
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("chmod work directory {}", path.display()))?;
    Ok(())
}

/// Send `signal` to `pid` through the `kill(2)` syscall.
///
/// Using the syscall instead of the `kill(1)` binary avoids a fork per call and
/// keeps expected failures out of the service log.
fn signal_process(pid: u32, signal: i32) -> std::io::Result<()> {
    // Reject pids that do not fit `pid_t` (and pid 0) rather than truncating:
    // a wrapped value would be a negative pid, which `kill(2)` reads as a
    // process *group*, and `kill(-1, …)` signals every process we may signal.
    // No live caller can reach that today — pids come from `/proc` and are
    // cross-checked against the pid file — but the cast must not be the thing
    // standing between a future caller and a signal to the whole system.
    let pid = libc::pid_t::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid pid"))?;
    // SAFETY: `kill` takes plain integers and shares no memory with the caller.
    let result = unsafe { libc::kill(pid, signal) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn terminate_process(pid: u32) -> Result<()> {
    signal_process(pid, libc::SIGTERM).with_context(|| format!("send SIGTERM to dae pid {pid}"))
}

fn force_kill_process(pid: u32) -> Result<()> {
    signal_process(pid, libc::SIGKILL).with_context(|| format!("send SIGKILL to dae pid {pid}"))
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
    fn compat_env_defaults_to_enabled_when_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("DAE_DISABLE_TCP_RELAY_OFFLOAD");
        std::env::remove_var("CHAOS_DAE_DISABLE_TCP_RELAY_OFFLOAD");
        assert_eq!(
            compat_env_value("DAE_DISABLE_TCP_RELAY_OFFLOAD"),
            Some("1".to_string())
        );
    }

    #[test]
    fn compat_env_is_inherited_when_already_set() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("DAE_DISABLE_TCP_RELAY_OFFLOAD", "0");
        std::env::remove_var("CHAOS_DAE_DISABLE_TCP_RELAY_OFFLOAD");
        assert_eq!(compat_env_value("DAE_DISABLE_TCP_RELAY_OFFLOAD"), None);
        std::env::remove_var("DAE_DISABLE_TCP_RELAY_OFFLOAD");
    }

    #[test]
    fn compat_env_honors_chaos_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("QUIC_GO_DISABLE_GSO");
        std::env::set_var("CHAOS_QUIC_GO_DISABLE_GSO", "0");
        assert_eq!(
            compat_env_value("QUIC_GO_DISABLE_GSO"),
            Some("0".to_string())
        );
        std::env::remove_var("CHAOS_QUIC_GO_DISABLE_GSO");
    }

    #[test]
    fn data_plane_status_reports_the_linux_dae_kind() {
        let status = data_plane_status();
        assert_eq!(status.kind, DATA_PLANE_KIND);
        assert_eq!(status.kind, "linux-dae");
    }

    #[test]
    fn proc_stat_state_parses_zombie_and_normal() {
        // Classic zombie line: `pid (comm) Z ...`
        assert_eq!(parse_proc_stat_state("123 (dae) Z 1 2 3"), Some('Z'));
        assert_eq!(parse_proc_stat_state("123 (dae) S 1 2 3"), Some('S'));
        // comm may contain spaces and parens; state is after the LAST `)`.
        assert_eq!(
            parse_proc_stat_state("456 (dae (with parens)) S 1 2 3"),
            Some('S')
        );
        assert_eq!(
            parse_proc_stat_state("789 (name with spaces) R 0 0"),
            Some('R')
        );
        // Malformed input: no closing paren / nothing after the comm.
        assert_eq!(parse_proc_stat_state("no closing paren"), None);
        assert_eq!(parse_proc_stat_state("123 (dae)"), None);
    }
}
