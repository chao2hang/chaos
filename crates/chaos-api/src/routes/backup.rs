//! Backup and restore: create/download/list/restore full system backups.

use std::path::PathBuf;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::auth::AuthUser;
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

/// List available backups.
async fn list_backups(_user: AuthUser) -> Result<Json<ListBackupsResponse>, ApiError> {
    let dir = backup_dir();
    let mut backups = Vec::new();

    if dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "tar" || ext == "gz") {
                    let metadata = std::fs::metadata(&path).ok();
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
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
    }

    // Sort by created_at descending
    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(Json(ListBackupsResponse { backups }))
}

/// Create a new backup of the database and dae config.
async fn create_backup(
    _user: AuthUser,
    RequestLocale(locale): RequestLocale,
) -> Result<Json<CreateBackupResponse>, ApiError> {
    let dir = backup_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| ApiError::internal_logged(locale, format!("create backup dir: {e}")))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("chaos_backup_{timestamp}.tar.gz");
    let backup_path = dir.join(&backup_name);

    // Use tar command to create the archive
    let db = db_path();
    let dae_dir = dae_work_dir();

    let mut files_to_backup: Vec<PathBuf> = Vec::new();
    if db.is_file() {
        files_to_backup.push(db.clone());
    }
    let config_path = dae_dir.join("config.dae");
    if config_path.is_file() {
        files_to_backup.push(config_path);
    }

    if files_to_backup.is_empty() {
        return Err(ApiError::bad_request("nothing_to_backup", locale));
    }

    // Create tar.gz using std::process::Command
    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&backup_path)
        .args(&files_to_backup)
        .status()
        .map_err(|e| ApiError::internal_logged(locale, format!("run tar: {e}")))?;

    if !status.success() {
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

pub fn backup_router() -> Router<AppState> {
    Router::new()
        .route("/backups", get(list_backups).post(create_backup))
}
