//! Self-update: check for new versions and apply updates.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AdminUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

const GITHUB_REPO: &str = "daeuniverse/dae";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Serialize)]
pub struct VersionInfo {
    pub current: String,
    pub latest: Option<String>,
    pub update_available: bool,
    pub dae_version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckUpdateResponse {
    pub version: VersionInfo,
}

#[derive(Debug, Deserialize)]
pub struct ApplyUpdateRequest {
    /// Target component: "chaos" or "dae"
    pub component: String,
}

#[derive(Debug, Serialize)]
pub struct ApplyUpdateResponse {
    pub ok: bool,
    pub message: String,
    pub restart_required: bool,
}

/// Get current version info and check for updates.
async fn check_update(
    _user: AdminUser,
    RequestLocale(locale): RequestLocale,
) -> Result<Json<CheckUpdateResponse>, ApiError> {
    let dae_version = read_dae_version();

    // For now, we don't actually check GitHub (would require async HTTP).
    // This is a placeholder that returns current version info.
    Ok(Json(CheckUpdateResponse {
        version: VersionInfo {
            current: CURRENT_VERSION.to_string(),
            latest: None,
            update_available: false,
            dae_version,
        },
    }))
}

/// Apply an update (placeholder - actual implementation would download and replace binaries).
async fn apply_update(
    _user: AdminUser,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<ApplyUpdateRequest>,
) -> Result<Json<ApplyUpdateResponse>, ApiError> {
    // This is a placeholder. Real implementation would:
    // 1. Download the new binary from GitHub releases
    // 2. Verify checksum
    // 3. Replace the binary
    // 4. Trigger systemd restart

    Ok(Json(ApplyUpdateResponse {
        ok: false,
        message: format!(
            "Update for '{}' is not yet implemented. Current version: {}",
            body.component, CURRENT_VERSION
        ),
        restart_required: false,
    }))
}

fn read_dae_version() -> Option<String> {
    let bin = chaos_dae::resolve_dae_bin()?;
    let output = std::process::Command::new(&bin)
        .arg("version")
        .output()
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().next().map(|l| l.trim().to_string())
    } else {
        None
    }
}

pub fn update_router() -> Router<AppState> {
    Router::new()
        .route("/update/check", get(check_update))
        .route("/update/apply", post(apply_update))
}
