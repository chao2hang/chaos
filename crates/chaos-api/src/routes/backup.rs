//! Backup and restore: create / list / download / restore system backups (admin only).

use std::path::{Path, PathBuf};

use axum::extract::Path as AxumPath;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AdminUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct BackupInfo {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ListBackupsResponse {
    pub backups: Vec<BackupInfo>,
}

#[derive(Debug, Serialize)]
pub struct CreateBackupResponse {
    pub backup: BackupInfo,
}

#[derive(Debug, Deserialize)]
pub struct RestoreRequest {
    /// Backup file name under the backup directory (e.g. chaos_backup_….tar.gz).
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct RestoreResponse {
    pub ok: bool,
    pub message: String,
}

fn backup_dir() -> PathBuf {
    std::env::var("CHAOS_BACKUP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./data/backups"))
}

fn db_path() -> PathBuf {
    std::env::var("CHAOS_DATABASE_URL")
        .ok()
        .and_then(|url| url.strip_prefix("sqlite:").map(String::from))
        .map(|p| p.split('?').next().unwrap_or(&p).to_string())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./data/chaos.db"))
}

fn dae_work_dir() -> PathBuf {
    std::env::var("CHAOS_DAE_WORK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./data/dae"))
}

fn is_safe_backup_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && (name.ends_with(".tar.gz") || name.ends_with(".tgz") || name.ends_with(".tar"))
}

/// List available backups.
async fn list_backups(_admin: AdminUser) -> Result<Json<ListBackupsResponse>, ApiError> {
    let dir = backup_dir();
    let mut backups = Vec::new();

    if dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !is_safe_backup_name(&name) {
                    continue;
                }
                let metadata = std::fs::metadata(&path).ok();
                backups.push(BackupInfo {
                    name: name.clone(),
                    path: path.display().to_string(),
                    bytes: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                    created_at: metadata
                        .and_then(|m| m.modified().ok())
                        .map(|t| {
                            t.duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0)
                                .to_string()
                        })
                        .unwrap_or_default(),
                });
            }
        }
    }

    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(Json(ListBackupsResponse { backups }))
}

/// Create a portable archive with relative members: `chaos.db` and optional `config.dae`.
async fn create_backup(
    _admin: AdminUser,
    RequestLocale(locale): RequestLocale,
) -> Result<Json<CreateBackupResponse>, ApiError> {
    let dir = backup_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| ApiError::internal_logged(locale, format!("create backup dir: {e}")))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("chaos_backup_{timestamp}.tar.gz");
    let backup_path = dir.join(&backup_name);

    let db = db_path();
    let dae_dir = dae_work_dir();
    let config_path = dae_dir.join("config.dae");

    let staging = tempfile_dir(&dir, locale)?;
    let staging_path = staging.clone();

    if db.is_file() {
        let dest = staging_path.join("chaos.db");
        std::fs::copy(&db, &dest)
            .map_err(|e| ApiError::internal_logged(locale, format!("copy db for backup: {e}")))?;
    }
    if config_path.is_file() {
        let dest = staging_path.join("config.dae");
        std::fs::copy(&config_path, &dest).map_err(|e| {
            ApiError::internal_logged(locale, format!("copy config.dae for backup: {e}"))
        })?;
    }

    let mut members: Vec<String> = Vec::new();
    if staging_path.join("chaos.db").is_file() {
        members.push("chaos.db".into());
    }
    if staging_path.join("config.dae").is_file() {
        members.push("config.dae".into());
    }
    if members.is_empty() {
        let _ = std::fs::remove_dir_all(&staging_path);
        return Err(ApiError::bad_request("nothing_to_backup", locale));
    }

    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&backup_path)
        .arg("-C")
        .arg(&staging_path)
        .args(&members)
        .status()
        .map_err(|e| ApiError::internal_logged(locale, format!("run tar: {e}")))?;

    let _ = std::fs::remove_dir_all(&staging_path);

    if !status.success() {
        let _ = std::fs::remove_file(&backup_path);
        return Err(ApiError::internal_logged(locale, "tar command failed"));
    }

    let metadata = std::fs::metadata(&backup_path)
        .map_err(|e| ApiError::internal_logged(locale, format!("stat backup: {e}")))?;

    Ok(Json(CreateBackupResponse {
        backup: BackupInfo {
            name: backup_name,
            path: backup_path.display().to_string(),
            bytes: metadata.len(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    }))
}

fn tempfile_dir(parent: &Path, locale: chaos_i18n::Locale) -> Result<PathBuf, ApiError> {
    let name = format!(
        ".staging_{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    let path = parent.join(name);
    std::fs::create_dir_all(&path)
        .map_err(|e| ApiError::internal_logged(locale, format!("create staging: {e}")))?;
    Ok(path)
}

/// Download a backup archive by safe file name.
async fn download_backup(
    _admin: AdminUser,
    RequestLocale(locale): RequestLocale,
    AxumPath(name): AxumPath<String>,
) -> Result<Response, ApiError> {
    if !is_safe_backup_name(&name) {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let path = backup_dir().join(&name);
    if !path.is_file() {
        return Err(ApiError::not_found("not_found", locale));
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| ApiError::internal_logged(locale, format!("read backup: {e}")))?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/gzip"),
            (
                header::CONTENT_DISPOSITION,
                &format!("attachment; filename=\"{name}\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// Restore DB and/or dae config from a backup archive.
/// Caller should restart chaos-api after restore so SQLite pool reloads the file.
async fn restore_backup(
    _admin: AdminUser,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<RestoreRequest>,
) -> Result<Json<RestoreResponse>, ApiError> {
    if !is_safe_backup_name(&body.name) {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let archive = backup_dir().join(&body.name);
    if !archive.is_file() {
        return Err(ApiError::not_found("not_found", locale));
    }

    let extract_dir = tempfile_dir(&backup_dir(), locale)?;
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&extract_dir)
        .status()
        .map_err(|e| ApiError::internal_logged(locale, format!("extract backup: {e}")))?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&extract_dir);
        return Err(ApiError::internal_logged(locale, "tar extract failed"));
    }

    let mut restored = Vec::new();
    let db_member = extract_dir.join("chaos.db");
    if db_member.is_file() {
        let dest = db_path();
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ApiError::internal_logged(locale, format!("mkdir db parent: {e}")))?;
        }
        // Replace DB file atomically where possible.
        let tmp = dest.with_extension("db.restoring");
        std::fs::copy(&db_member, &tmp)
            .map_err(|e| ApiError::internal_logged(locale, format!("stage db: {e}")))?;
        std::fs::rename(&tmp, &dest)
            .map_err(|e| ApiError::internal_logged(locale, format!("replace db: {e}")))?;
        restored.push("chaos.db");
    }

    let cfg_member = extract_dir.join("config.dae");
    if cfg_member.is_file() {
        let work = dae_work_dir();
        std::fs::create_dir_all(&work)
            .map_err(|e| ApiError::internal_logged(locale, format!("mkdir dae work: {e}")))?;
        let dest = work.join("config.dae");
        std::fs::copy(&cfg_member, &dest)
            .map_err(|e| ApiError::internal_logged(locale, format!("restore config.dae: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600));
        }
        restored.push("config.dae");
    }

    let _ = std::fs::remove_dir_all(&extract_dir);

    if restored.is_empty() {
        return Err(ApiError::bad_request("invalid_dae_config", locale));
    }

    Ok(Json(RestoreResponse {
        ok: true,
        message: format!(
            "Restored {}. Restart chaos-api (or systemctl restart chaos) to reload the database pool, then Apply if needed.",
            restored.join(", ")
        ),
    }))
}

pub fn backup_router() -> Router<AppState> {
    Router::new()
        .route("/backups", get(list_backups).post(create_backup))
        .route("/backups/{name}/download", get(download_backup))
        .route("/backups/restore", post(restore_backup))
}
