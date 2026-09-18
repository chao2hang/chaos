//! Self-update: check GitHub releases, install packages, restart, and roll back on failure.

use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::auth::AdminUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

const GITHUB_REPO: &str = "chao2hang/chaos";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const USER_AGENT: &str = "chaos-self-update";

/// Shared clients for the release API / checksums and for the package download.
/// Both were rebuilt per call, discarding the connection pool and TLS cache.
static GITHUB_CLIENT: std::sync::LazyLock<Option<reqwest::Client>> =
    std::sync::LazyLock::new(|| crate::http::build_client(HTTP_TIMEOUT, USER_AGENT));
static DOWNLOAD_CLIENT: std::sync::LazyLock<Option<reqwest::Client>> =
    std::sync::LazyLock::new(|| crate::http::build_client(DOWNLOAD_TIMEOUT, USER_AGENT));
const UPDATE_DIR: &str = "/var/lib/chaos/updates";
const STATUS_FILE: &str = "/var/lib/chaos/update-status.json";
const SERVICE_NAME: &str = "chaos.service";

#[derive(Debug, Clone, Serialize)]
pub struct VersionInfo {
    pub current: String,
    pub latest: Option<String>,
    pub update_available: bool,
    pub release_url: Option<String>,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
    pub dae_version: Option<String>,
    /// Non-empty when the update check could not reach GitHub. Present so the
    /// UI can show "version unknown / check unavailable" instead of an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub phase: String,
    pub target_version: Option<String>,
    pub message: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

impl Default for UpdateStatus {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            target_version: None,
            message: None,
            started_at: None,
            finished_at: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CheckUpdateResponse {
    pub version: VersionInfo,
    pub status: UpdateStatus,
}

#[derive(Debug, Deserialize, Default)]
pub struct ApplyUpdateRequest {
    #[serde(default)]
    pub component: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApplyUpdateResponse {
    pub ok: bool,
    pub message: String,
    pub restart_required: bool,
    pub status: UpdateStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Last successful release check, cached on disk so a transient GitHub outage
/// (or a firewall/GFW blocking api.github.com) degrades to stale data instead
/// of a 500.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedRelease {
    fetched_at_unix: u64,
    release: GithubRelease,
}

/// Where the last successful release check is cached. Overridable via
/// `CHAOS_UPDATE_CACHE_FILE` (used by tests).
const UPDATE_CACHE_FILE: &str = "/var/lib/chaos/update-check-cache.json";
/// A cached release older than this is not served.
const UPDATE_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// `CHAOS_DISABLE_RELEASE_CHECK=1` turns off all outbound GitHub release
/// checks (for fully offline / firewalled deployments).
fn release_check_disabled() -> bool {
    matches!(
        std::env::var("CHAOS_DISABLE_RELEASE_CHECK")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    )
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn update_cache_path() -> PathBuf {
    std::env::var_os("CHAOS_UPDATE_CACHE_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(UPDATE_CACHE_FILE))
}

fn write_cached_release(release: &GithubRelease) {
    let cached = CachedRelease {
        fetched_at_unix: unix_now(),
        release: release.clone(),
    };
    let path = update_cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(&cached) {
        let _ = std::fs::write(path, bytes);
    }
}

/// Read the cached release if it is fresh enough to serve. `None` when absent,
/// stale, or unreadable.
async fn read_cached_release() -> Option<GithubRelease> {
    let (fetched_at_unix, release) = read_cached_release_any_age().await?;
    (unix_now().saturating_sub(fetched_at_unix) <= UPDATE_CACHE_TTL.as_secs()).then_some(release)
}

/// Read the cached release regardless of age, with its fetch timestamp.
///
/// Uses `tokio::fs` rather than `std::fs`: this runs on every `/update/check`
/// poll, which means on a request-handling worker thread. (The matching write
/// still uses `std::fs` because it happens once per refresh, not per request.)
async fn read_cached_release_any_age() -> Option<(u64, GithubRelease)> {
    let raw = tokio::fs::read_to_string(update_cache_path()).await.ok()?;
    let cached: CachedRelease = serde_json::from_str(&raw).ok()?;
    Some((cached.fetched_at_unix, cached.release))
}

pub fn update_router() -> Router<AppState> {
    Router::new()
        .route("/update/check", get(check_update))
        .route("/update/apply", post(apply_update))
        .route("/update/status", get(update_status))
}

/// Query for `/update/check`. The console's explicit "Check for updates"
/// button asks for `force`, because the automatic call on page load serves the
/// cache — without an escape hatch a release published since the last fetch
/// would keep reporting "up to date" for [`UPDATE_CACHE_TTL`].
#[derive(Debug, Deserialize)]
pub struct CheckUpdateQuery {
    /// Accepts `1`/`true`/`yes`/`on` rather than only serde's `true`/`false`,
    /// so a hand-written URL cannot turn the request into a confusing 400.
    #[serde(default, deserialize_with = "deserialize_flag")]
    force: bool,
}

fn deserialize_flag<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    let raw = String::deserialize(deserializer)?;
    Ok(matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    ))
}

async fn check_update(
    _user: AdminUser,
    RequestLocale(locale): RequestLocale,
    Query(query): Query<CheckUpdateQuery>,
) -> Result<Json<CheckUpdateResponse>, ApiError> {
    if release_check_disabled() {
        return Ok(Json(CheckUpdateResponse {
            version: version_info(None, None, (None, None), read_dae_version_offloaded().await),
            status: read_status().await,
        }));
    }

    // Serve a fresh cached result before calling GitHub: the unauthenticated
    // release API allows only 60 requests per hour per address, and this
    // endpoint is polled from the console. A stale entry is still better than
    // nothing when the network is blocked, so it is the fallback on failure.
    let fresh_cache = if query.force {
        None
    } else {
        read_cached_release().await
    };
    let (latest, error) = match fresh_cache {
        Some(cached) => (Some(cached), None),
        None => match fetch_latest_release(locale).await {
            Ok(release) => {
                write_cached_release(&release);
                (Some(release), None)
            }
            Err(failure) => match read_cached_release_any_age().await {
                Some((_, cached)) => (
                    Some(cached),
                    Some("update check failed; showing cached result".to_string()),
                ),
                None => (None, Some(format!("update check unavailable: {failure}"))),
            },
        },
    };

    Ok(Json(CheckUpdateResponse {
        version: version_info(
            latest.as_ref(),
            error,
            latest
                .as_ref()
                .map(|release| select_asset(&release.assets))
                .unwrap_or((None, None)),
            read_dae_version_offloaded().await,
        ),
        status: read_status().await,
    }))
}

/// `dae version` forks a process, so keep it off the async worker threads.
/// A failed task only costs the displayed version, so it is logged, not raised.
async fn read_dae_version_offloaded() -> Option<String> {
    match tokio::task::spawn_blocking(read_dae_version).await {
        Ok(version) => version,
        Err(error) => {
            tracing::warn!(%error, "dae version probe task failed");
            None
        }
    }
}

/// Build a `VersionInfo` from an optional fetched release. When `release` is
/// `None` the result reports "version unknown" (no update available).
fn version_info(
    release: Option<&GithubRelease>,
    error: Option<String>,
    (download_url, asset_name): (Option<String>, Option<String>),
    dae_version: Option<String>,
) -> VersionInfo {
    let latest_version =
        release.map(|release| release.tag_name.trim_start_matches('v').to_string());
    VersionInfo {
        current: CURRENT_VERSION.to_string(),
        latest: latest_version.clone(),
        update_available: latest_version
            .as_deref()
            .is_some_and(|version| newer_version(version, CURRENT_VERSION)),
        release_url: release.map(|release| release.html_url.clone()),
        download_url,
        asset_name,
        dae_version,
        error,
    }
}

async fn update_status(_user: AdminUser) -> Result<Json<UpdateStatus>, ApiError> {
    Ok(Json(read_status().await))
}

async fn apply_update(
    _user: AdminUser,
    RequestLocale(locale): RequestLocale,
    State(state): State<AppState>,
    Json(body): Json<ApplyUpdateRequest>,
) -> Result<Json<ApplyUpdateResponse>, ApiError> {
    if body
        .component
        .as_deref()
        .is_some_and(|c| !c.eq_ignore_ascii_case("chaos"))
    {
        return Err(ApiError::bad_request(
            "unsupported_update_component",
            locale,
        ));
    }
    if !cfg!(target_os = "linux") || !systemd_available() {
        return Err(ApiError::bad_request("update_requires_systemd", locale));
    }

    let _guard = state.update_lock.lock().await;
    let status = read_status().await;
    if matches!(
        status.phase.as_str(),
        "downloading" | "installing" | "restarting" | "verifying"
    ) {
        return Err(ApiError::conflict("update_in_progress", locale));
    }

    let release = fetch_latest_release(locale).await?;
    let latest_version = release.tag_name.trim_start_matches('v').to_string();
    let target = body.version.unwrap_or_else(|| latest_version.clone());
    if !target.is_empty() && target != latest_version {
        return Err(ApiError::bad_request("update_version_not_found", locale));
    }
    if !newer_version(&target, CURRENT_VERSION) {
        return Err(ApiError::bad_request("update_not_available", locale));
    }

    let (download_url, asset_name) = match select_asset(&release.assets) {
        (Some(url), Some(name)) => (url, name),
        _ => return Err(ApiError::bad_request("update_asset_not_found", locale)),
    };
    let checksum_url = select_checksum(&release.assets);
    ensure_update_dir(locale)?;
    let deb_path = PathBuf::from(UPDATE_DIR).join(&asset_name);
    let started_at = chaos_store::now_rfc3339();
    write_status(&UpdateStatus {
        phase: "downloading".into(),
        target_version: Some(target.clone()),
        message: Some(format!("Downloading {}", asset_name)),
        started_at: Some(started_at.clone()),
        finished_at: None,
    })?;

    if let Err(err) = download_file(&download_url, &deb_path).await {
        write_status(&UpdateStatus {
            phase: "failed".into(),
            target_version: Some(target),
            message: Some(err.clone()),
            started_at: Some(started_at),
            finished_at: Some(chaos_store::now_rfc3339()),
        })?;
        return Err(ApiError::internal_logged(locale, err));
    }

    if let Some(url) = checksum_url {
        if let Err(err) = verify_checksum(&deb_path, &asset_name, &url).await {
            write_status(&UpdateStatus {
                phase: "failed".into(),
                target_version: Some(target),
                message: Some(err.clone()),
                started_at: Some(started_at),
                finished_at: Some(chaos_store::now_rfc3339()),
            })?;
            return Err(ApiError::internal_logged(locale, err));
        }
    }

    let script = make_update_script(&deb_path, &asset_name, &target, &started_at);
    let script_path = PathBuf::from(UPDATE_DIR).join("apply-update.sh");
    write_script(&script_path, &script)?;
    spawn_detached_update(&script_path)
        .map_err(|err| ApiError::internal_logged(locale, format!("spawn update helper: {err}")))?;

    Ok(Json(ApplyUpdateResponse {
        ok: true,
        message: "Update started; chaos will restart shortly.".into(),
        restart_required: true,
        status: UpdateStatus {
            phase: "installing".into(),
            target_version: Some(target),
            message: Some("Installing update and restarting service".into()),
            started_at: Some(started_at),
            finished_at: None,
        },
    }))
}

async fn fetch_latest_release(locale: chaos_i18n::Locale) -> Result<GithubRelease, ApiError> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let Some(client) = &*GITHUB_CLIENT else {
        return Err(ApiError::update_check_failed(
            locale,
            "http client unavailable".to_string(),
        ));
    };
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| ApiError::update_check_failed(locale, format!("fetch release: {err}")))?;
    if !response.status().is_success() {
        return Err(ApiError::update_check_failed(
            locale,
            format!("GitHub release API returned {}", response.status()),
        ));
    }
    response
        .json()
        .await
        .map_err(|err| ApiError::update_check_failed(locale, format!("parse release: {err}")))
}

fn select_asset(assets: &[GithubAsset]) -> (Option<String>, Option<String>) {
    let wanted = match std::env::consts::ARCH {
        "x86_64" => "_amd64.deb",
        "aarch64" => "_arm64.deb",
        other => {
            tracing::warn!(arch = other, "unsupported self-update architecture");
            return (None, None);
        }
    };
    assets
        .iter()
        .find(|a| a.name.ends_with(wanted))
        .map(|a| (Some(a.browser_download_url.clone()), Some(a.name.clone())))
        .unwrap_or((None, None))
}

fn select_checksum(assets: &[GithubAsset]) -> Option<String> {
    assets
        .iter()
        .find(|a| a.name == "SHA256SUMS")
        .map(|a| a.browser_download_url.clone())
}

async fn verify_checksum(path: &Path, asset_name: &str, url: &str) -> Result<(), String> {
    let Some(client) = &*GITHUB_CLIENT else {
        return Err("http client unavailable".into());
    };
    let body = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("download checksums: {err}"))?
        .text()
        .await
        .map_err(|err| format!("read checksums: {err}"))?;
    let expected = parse_checksum_line(&body, asset_name)
        .ok_or_else(|| format!("checksum missing for {asset_name}"))?;

    let bytes = tokio::fs::read(path)
        .await
        .map_err(|err| format!("read package: {err}"))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        return Err(format!(
            "checksum mismatch for {asset_name}: expected {expected}, got {actual}"
        ));
    }
    Ok(())
}

fn parse_checksum_line(body: &str, asset_name: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let digest = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        (file == asset_name).then_some(digest.to_ascii_lowercase())
    })
}

fn newer_version(candidate: &str, current: &str) -> bool {
    parse_version(candidate)
        .zip(parse_version(current))
        .map(|(c, cur)| c > cur)
        .unwrap_or(false)
}

fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let core = version
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or(version);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// Stream the package to `dest` instead of buffering it: the .deb is tens of
/// megabytes, and holding a second copy in memory while the first is written
/// only adds peak RSS pressure on a small host.
async fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let Some(client) = &*DOWNLOAD_CLIENT else {
        return Err("http client unavailable".into());
    };
    match stream_to_file(client, url, dest).await {
        Ok(()) => Ok(()),
        Err(error) => {
            // A truncated .deb must not be left behind: the install step would
            // find a file of plausible size, and only the checksum (computed
            // later, from the same directory) would catch it.
            let _ = tokio::fs::remove_file(dest).await;
            Err(error)
        }
    }
}

async fn stream_to_file(client: &reqwest::Client, url: &str, dest: &Path) -> Result<(), String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("download package: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "download package failed: HTTP {}",
            response.status()
        ));
    }

    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|err| format!("create {}: {err}", dest.display()))?;
    let mut written: u64 = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| format!("read package: {err}"))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|err| format!("write package: {err}"))?;
        written += u64::try_from(chunk.len()).unwrap_or(u64::MAX);
    }
    file.flush()
        .await
        .map_err(|err| format!("flush package: {err}"))?;
    if written < 1024 {
        return Err("downloaded package is too small".into());
    }
    Ok(())
}

fn ensure_update_dir(locale: chaos_i18n::Locale) -> Result<(), ApiError> {
    std::fs::create_dir_all(UPDATE_DIR)
        .map_err(|err| ApiError::internal_logged(locale, format!("create update dir: {err}")))
}

/// The update status file, or the default "idle" state when absent/unreadable.
///
/// Reads with `tokio::fs` for the same reason as the release cache: this is
/// reached from request handlers.
async fn read_status() -> UpdateStatus {
    match tokio::fs::read(STATUS_FILE).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => UpdateStatus::default(),
    }
}

fn write_status(status: &UpdateStatus) -> Result<(), ApiError> {
    if let Some(parent) = Path::new(STATUS_FILE).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let bytes = serde_json::to_vec_pretty(status).map_err(|err| {
        ApiError::internal_logged(chaos_i18n::Locale::En, format!("serialize status: {err}"))
    })?;
    std::fs::write(STATUS_FILE, bytes).map_err(|err| {
        ApiError::internal_logged(chaos_i18n::Locale::En, format!("write status: {err}"))
    })
}

fn write_script(path: &Path, content: &str) -> Result<(), ApiError> {
    std::fs::write(path, content)
        .map_err(|err| ApiError::internal_logged(chaos_i18n::Locale::En, err))?;
    set_executable(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), ApiError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(path, mode)
        .map_err(|err| ApiError::internal_logged(chaos_i18n::Locale::En, err))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), ApiError> {
    Ok(())
}

fn systemd_available() -> bool {
    which("systemctl")
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// Start the update script in its own session so it survives the API restart
/// it is about to trigger.
#[cfg(unix)]
fn spawn_detached_update(script: &Path) -> Result<(), String> {
    use std::process::Stdio;

    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(UPDATE_DIR).join("update.log"))
        .map_err(|err| format!("open update log: {err}"))?;

    let err = log.try_clone().map_err(|err| format!("clone log: {err}"))?;
    let mut cmd = tokio::process::Command::new("/bin/sh");
    cmd.arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .kill_on_drop(false);
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn().map_err(|err| format!("spawn helper: {err}"))?;
    Ok(())
}

/// The update helper is a POSIX shell script driven by systemd; `apply_update`
/// rejects non-Linux targets before reaching here, so this is unreachable.
#[cfg(not(unix))]
fn spawn_detached_update(_script: &Path) -> Result<(), String> {
    Err("self-update is only supported on Linux".to_string())
}

fn make_update_script(deb_path: &Path, asset_name: &str, target: &str, started_at: &str) -> String {
    let pkg_dir = "/usr/lib/chaos";
    let web_dir = "/usr/share/chaos/web";
    let backup_root = format!("{UPDATE_DIR}/backup/{target}-{started_at}");
    let deb_path = deb_path.display();
    let update_dir = UPDATE_DIR;
    let status_file = STATUS_FILE;
    let service = SERVICE_NAME;

    format!(
        r#"#!/bin/sh
set -eu
UPDATE_DIR="{update_dir}"
STATUS_FILE="{status_file}"
SERVICE="{service}"
TARGET="{target}"
DEB="{deb_path}"
ASSET="{asset_name}"
BACKUP_DIR="{backup_root}"
PKG_DIR="{pkg_dir}"
WEB_DIR="{web_dir}"

write_status() {{
  phase="$1"
  message="$2"
  finished="$3"
  finished_json="null"
  if [ -n "$finished" ]; then
    finished_json="\"$finished\""
  fi
  cat > "$STATUS_FILE" <<EOF
{{
  "phase": "$phase",
  "target_version": "$TARGET",
  "message": "$message",
  "started_at": "{started_at}",
  "finished_at": $finished_json
}}
EOF
}}

cleanup_artifacts() {{
  # The downloaded package and the rollback snapshot are only needed until the
  # new version passes its health check. Keeping them forever accumulated
  # roughly 40 MB per update on hosts with limited disk.
  rm -f "$DEB"
  rm -rf "$BACKUP_DIR" "$UPDATE_DIR/backup"
  # Sweep packages left behind by earlier attempts, including ones from
  # versions that predate this cleanup.
  find "$UPDATE_DIR" -maxdepth 1 -type f -name 'chaos_*.deb' -exec rm -f {{}} + 2>/dev/null || true
}}

restore_backup() {{
  if [ -f "$BACKUP_DIR/chaos-api" ]; then
    cp -a "$BACKUP_DIR/chaos-api" "$PKG_DIR/bin/chaos-api"
  fi
  if [ -f "$BACKUP_DIR/chaos-prober" ]; then
    cp -a "$BACKUP_DIR/chaos-prober" "$PKG_DIR/bin/chaos-prober"
  fi
  if [ -d "$BACKUP_DIR/web" ]; then
    rm -rf "$WEB_DIR"
    cp -a "$BACKUP_DIR/web" "$WEB_DIR"
  fi
}}

write_status "installing" "Backing up current installation" ""
mkdir -p "$BACKUP_DIR/web"
[ -f "$PKG_DIR/bin/chaos-api" ] && cp -a "$PKG_DIR/bin/chaos-api" "$BACKUP_DIR/chaos-api"
[ -f "$PKG_DIR/bin/chaos-prober" ] && cp -a "$PKG_DIR/bin/chaos-prober" "$BACKUP_DIR/chaos-prober"
[ -d "$WEB_DIR" ] && cp -a "$WEB_DIR/." "$BACKUP_DIR/web/"

write_status "installing" "Installing $ASSET" ""
if ! dpkg -i "$DEB"; then
  write_status "failed" "dpkg install failed" "$(date -Iseconds)"
  restore_backup
  systemctl restart "$SERVICE" || true
  write_status "rolled_back" "Install failed; previous version restored" "$(date -Iseconds)"
  exit 1
fi

write_status "restarting" "Restarting chaos service" ""
systemctl restart "$SERVICE"

write_status "verifying" "Waiting for health check" ""
i=0
while [ "$i" -lt 20 ]; do
  i=$((i + 1))
  sleep 2
  if health=$(curl -fsS --max-time 3 "http://127.0.0.1:2030/api/v1/health" 2>/dev/null); then
    case "$health" in
      *"\"api_version\":\"$TARGET\""*)
        write_status "completed" "Update completed and health check passed" "$(date -Iseconds)"
        cleanup_artifacts
        exit 0
        ;;
    esac
  fi
done

write_status "failed" "Health check failed after update; rolling back" "$(date -Iseconds)"
restore_backup
systemctl restart "$SERVICE" || true
write_status "rolled_back" "Health check failed; previous version restored" "$(date -Iseconds)"
exit 1
"#
    )
}

fn read_dae_version() -> Option<String> {
    let bin = chaos_dae::resolve_dae_bin()?;
    let output = std::process::Command::new(bin)
        .arg("version")
        .output()
        .ok()?;
    if output.status.success() {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .map(|line| line.trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("v0.1.4"), Some((0, 1, 4)));
        assert_eq!(parse_version("0.2.0-beta"), Some((0, 2, 0)));
        assert!(newer_version("0.1.5", "0.1.4"));
        assert!(!newer_version("0.1.4", "0.1.4"));
    }

    #[test]
    fn selects_deb_asset_for_architecture() {
        let assets = vec![
            GithubAsset {
                name: "chaos_0.1.4_arm64.deb".into(),
                browser_download_url: "https://example/arm64".into(),
            },
            GithubAsset {
                name: "chaos_0.1.4_amd64.deb".into(),
                browser_download_url: "https://example/amd64".into(),
            },
        ];
        let (url, name) = select_asset(&assets);
        let expected_suffix = match std::env::consts::ARCH {
            "x86_64" => "amd64.deb",
            "aarch64" => "arm64.deb",
            other => panic!("unsupported arch {other}"),
        };
        assert!(name.unwrap().ends_with(expected_suffix));
        assert!(url.unwrap().starts_with("https://example/"));
    }

    #[test]
    fn version_comparison_respects_semver_ordering() {
        // Higher patch / minor / major is newer.
        assert!(newer_version("0.1.5", "0.1.4"));
        assert!(newer_version("0.2.0", "0.1.9"));
        assert!(newer_version("1.0.0", "0.9.9"));
        // Equal or older is not newer.
        assert!(!newer_version("0.1.4", "0.1.4"));
        assert!(!newer_version("0.1.3", "0.1.4"));
        // Leading v and pre-release suffixes are tolerated.
        assert!(newer_version("v0.2.0", "0.1.0"));
        assert!(newer_version("0.2.0-rc1", "0.1.0"));
        // Garbage versions never count as newer (fail safe: no update).
        assert!(!newer_version("latest", "0.1.4"));
        assert!(!newer_version("", "0.1.4"));
    }

    #[test]
    fn parse_checksum_line_supports_binary_and_text_modes() {
        let body = "deadbeef  chaos_0.1.5_amd64.deb\ncafebabe *chaos_0.1.5_arm64.deb\n";
        assert_eq!(
            parse_checksum_line(body, "chaos_0.1.5_amd64.deb").as_deref(),
            Some("deadbeef")
        );
        // GNU coreutils prepends '*' for binary mode.
        assert_eq!(
            parse_checksum_line(body, "chaos_0.1.5_arm64.deb").as_deref(),
            Some("cafebabe")
        );
        assert_eq!(parse_checksum_line(body, "missing.deb"), None);
    }

    #[test]
    fn select_checksum_finds_sha256sums_asset() {
        let assets = vec![
            GithubAsset {
                name: "chaos_0.1.5_amd64.deb".into(),
                browser_download_url: "https://example/deb".into(),
            },
            GithubAsset {
                name: "SHA256SUMS".into(),
                browser_download_url: "https://example/sums".into(),
            },
        ];
        assert_eq!(
            select_checksum(&assets).as_deref(),
            Some("https://example/sums")
        );
    }

    // ------------------------------------------------------------------
    // Update-check degradation: cache + disable switch + version_info
    // ------------------------------------------------------------------

    // Serialize env-mutating tests (set_var/remove_var are global). This is an
    // async-aware lock because some of them await the code under test while
    // holding it; a std guard across an await would be the wrong primitive and
    // trips `clippy::await_holding_lock`.
    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn sample_release() -> GithubRelease {
        GithubRelease {
            tag_name: "v0.2.0".into(),
            html_url: "https://github.com/chao2hang/chaos/releases/tag/v0.2.0".into(),
            assets: vec![
                GithubAsset {
                    name: "chaos_0.2.0_amd64.deb".into(),
                    browser_download_url: "https://example/amd64.deb".into(),
                },
                GithubAsset {
                    name: "SHA256SUMS".into(),
                    browser_download_url: "https://example/SHA256SUMS".into(),
                },
            ],
        }
    }

    fn cache_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("chaos-update-cache-{}", std::process::id()))
    }

    #[tokio::test]
    async fn cached_release_round_trips() {
        let _guard = ENV_LOCK.lock().await;
        let dir = cache_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cache.json");
        std::env::set_var("CHAOS_UPDATE_CACHE_FILE", &path);

        write_cached_release(&sample_release());
        let cached = read_cached_release()
            .await
            .expect("fresh cache should be served");
        assert_eq!(cached.tag_name, "v0.2.0");
        assert_eq!(cached.assets.len(), 2);

        std::env::remove_var("CHAOS_UPDATE_CACHE_FILE");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn stale_cache_is_not_served() {
        let _guard = ENV_LOCK.lock().await;
        let dir = cache_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stale.json");
        std::env::set_var("CHAOS_UPDATE_CACHE_FILE", &path);

        let stale = CachedRelease {
            fetched_at_unix: unix_now().saturating_sub(UPDATE_CACHE_TTL.as_secs() + 1),
            release: sample_release(),
        };
        std::fs::write(&path, serde_json::to_vec(&stale).unwrap()).unwrap();
        assert!(read_cached_release().await.is_none());

        std::env::remove_var("CHAOS_UPDATE_CACHE_FILE");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn release_check_disabled_honors_env() {
        let _guard = ENV_LOCK.lock().await;
        for value in ["1", "true", "yes", "TRUE"] {
            std::env::set_var("CHAOS_DISABLE_RELEASE_CHECK", value);
            assert!(release_check_disabled(), "value {value} should disable");
        }
        for value in ["0", "false", ""] {
            std::env::set_var("CHAOS_DISABLE_RELEASE_CHECK", value);
            assert!(
                !release_check_disabled(),
                "value {value:?} should not disable"
            );
        }
        std::env::remove_var("CHAOS_DISABLE_RELEASE_CHECK");
        assert!(!release_check_disabled());
    }

    #[test]
    fn check_update_query_parses_the_force_flag() {
        // `?force=1` is what the console's explicit "Check for updates" button
        // sends; the automatic call omits it and must serve the cache.
        for forced in ["1", "true", "TRUE", "yes", "on"] {
            let uri: axum::http::Uri = format!("/api/v1/update/check?force={forced}")
                .parse()
                .unwrap();
            let Query(query) = Query::<CheckUpdateQuery>::try_from_uri(&uri)
                .unwrap_or_else(|_| panic!("force={forced} should parse"));
            assert!(query.force, "force={forced} should force a live check");
        }

        for plain in ["", "?force=0", "?force=false", "?force=no"] {
            let uri: axum::http::Uri = format!("/api/v1/update/check{plain}").parse().unwrap();
            let Query(query) = Query::<CheckUpdateQuery>::try_from_uri(&uri).unwrap();
            assert!(!query.force, "{plain} should serve the cached result");
        }
    }

    #[test]
    fn version_info_reports_unknown_when_check_fails() {
        let info = version_info(None, Some("network down".into()), (None, None), None);
        assert!(info.latest.is_none());
        assert!(!info.update_available);
        assert_eq!(info.error.as_deref(), Some("network down"));
    }

    #[test]
    fn version_info_reports_update_from_release() {
        let release = sample_release();
        let (url, name) = select_asset(&release.assets);
        let info = version_info(Some(&release), None, (url, name), Some("v0.2.2".into()));
        assert_eq!(info.latest.as_deref(), Some("0.2.0"));
        assert!(info.update_available);
        assert!(info.error.is_none());
        assert_eq!(
            info.download_url.as_deref(),
            Some("https://example/amd64.deb")
        );
        assert_eq!(info.dae_version.as_deref(), Some("v0.2.2"));
    }

    #[tokio::test]
    async fn cached_release_is_served_only_while_fresh() {
        let _guard = ENV_LOCK.lock().await;
        let dir = std::env::temp_dir().join(format!("chaos-update-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cache = dir.join("release.json");
        std::env::set_var("CHAOS_UPDATE_CACHE_FILE", &cache);

        // Nothing cached yet.
        assert!(read_cached_release().await.is_none());
        assert!(read_cached_release_any_age().await.is_none());

        // A just-written entry is served without calling GitHub.
        let release = sample_release();
        write_cached_release(&release);
        assert!(read_cached_release().await.is_some());
        let (fetched_at, cached) = read_cached_release_any_age()
            .await
            .expect("entry written above");
        assert_eq!(cached.tag_name, release.tag_name);
        assert!(unix_now().saturating_sub(fetched_at) < 5);

        // A stale entry is not served as fresh, but remains available as the
        // fallback when the release check fails.
        let stale = CachedRelease {
            fetched_at_unix: unix_now() - UPDATE_CACHE_TTL.as_secs() - 60,
            release,
        };
        std::fs::write(&cache, serde_json::to_vec(&stale).unwrap()).unwrap();
        assert!(read_cached_release().await.is_none());
        assert!(read_cached_release_any_age().await.is_some());

        std::env::remove_var("CHAOS_UPDATE_CACHE_FILE");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
