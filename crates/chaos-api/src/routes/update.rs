//! Self-update: check GitHub releases, install packages, restart, and roll back on failure.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg_attr(not(target_os = "linux"), allow(unused_imports))]
#[cfg(unix)]
use std::os::unix::process::CommandExt as _;

use crate::auth::AdminUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

const GITHUB_REPO: &str = "chao2hang/chaos";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
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

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

pub fn update_router() -> Router<AppState> {
    Router::new()
        .route("/update/check", get(check_update))
        .route("/update/apply", post(apply_update))
        .route("/update/status", get(update_status))
}

async fn check_update(
    _user: AdminUser,
    RequestLocale(locale): RequestLocale,
) -> Result<Json<CheckUpdateResponse>, ApiError> {
    let latest = fetch_latest_release(locale).await?;
    let latest_version = latest.tag_name.trim_start_matches('v').to_string();
    let (download_url, asset_name) = select_asset(&latest.assets);
    let update_available = newer_version(&latest_version, CURRENT_VERSION);

    Ok(Json(CheckUpdateResponse {
        version: VersionInfo {
            current: CURRENT_VERSION.to_string(),
            latest: Some(latest_version),
            update_available,
            release_url: Some(latest.html_url),
            download_url,
            asset_name,
            dae_version: read_dae_version(),
        },
        status: read_status(),
    }))
}

async fn update_status(_user: AdminUser) -> Result<Json<UpdateStatus>, ApiError> {
    Ok(Json(read_status()))
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
    let status = read_status();
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
    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent("chaos-self-update")
        .build()
        .map_err(|err| ApiError::internal_logged(locale, err))?;
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| ApiError::internal_logged(locale, format!("fetch release: {err}")))?;
    if !response.status().is_success() {
        return Err(ApiError::internal_logged(
            locale,
            format!("GitHub release API returned {}", response.status()),
        ));
    }
    response
        .json()
        .await
        .map_err(|err| ApiError::internal_logged(locale, format!("parse release: {err}")))
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
    let body = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent("chaos-self-update")
        .build()
        .map_err(|err| format!("build checksum client: {err}"))?
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

async fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .user_agent("chaos-self-update")
        .build()
        .map_err(|err| format!("build download client: {err}"))?
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
    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("read package: {err}"))?;
    if bytes.len() < 1024 {
        return Err("downloaded package is too small".into());
    }
    tokio::fs::write(dest, &bytes)
        .await
        .map_err(|err| format!("write package: {err}"))
}

fn ensure_update_dir(locale: chaos_i18n::Locale) -> Result<(), ApiError> {
    std::fs::create_dir_all(UPDATE_DIR)
        .map_err(|err| ApiError::internal_logged(locale, format!("create update dir: {err}")))
}

fn read_status() -> UpdateStatus {
    std::fs::read(STATUS_FILE)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
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

fn spawn_detached_update(script: &Path) -> Result<(), String> {
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
}
