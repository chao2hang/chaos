//! Backup and restore: create / list / download / restore system backups (admin only).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use axum::body::{Body, Bytes};
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::header;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::io::AsyncReadExt;

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
///
/// The database member is produced with `VACUUM INTO` rather than `fs::copy`. A
/// byte copy taken while the service keeps writing can catch a torn page mid
/// checkpoint, and copying the main file alone silently drops every transaction
/// still sitting in the write-ahead log.
async fn create_backup(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
) -> Result<Json<CreateBackupResponse>, ApiError> {
    let dir = backup_dir();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| ApiError::internal_logged(locale, format!("create backup dir: {e}")))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("chaos_backup_{timestamp}.tar.gz");

    let staging = tempfile_dir(&dir, locale)?;
    if db_path().is_file() {
        let snapshot = staging.join("chaos.db");
        if let Err(error) = snapshot_database(&state.pool, &snapshot).await {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            return Err(ApiError::internal_logged(locale, error));
        }
    }

    let config_path = dae_work_dir().join("config.dae");
    if config_path.is_file() {
        if let Err(error) = tokio::fs::copy(&config_path, staging.join("config.dae")).await {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            return Err(ApiError::internal_logged(
                locale,
                format!("copy config.dae for backup: {error}"),
            ));
        }
    }

    // Gzipping the archive is a blocking call that can take seconds on a large
    // database, so keep it off the async worker threads.
    tokio::task::spawn_blocking(move || archive_backup(&dir, &staging, backup_name, locale))
        .await
        .map_err(|error| {
            ApiError::internal_logged(locale, format!("backup task failed: {error}"))
        })?
}

/// Write a consistent snapshot of the live database to `dest`.
async fn snapshot_database(pool: &SqlitePool, dest: &Path) -> Result<(), String> {
    // `VACUUM INTO` refuses to overwrite an existing file; the staging directory
    // is freshly created, so `dest` never exists here.
    let path = dest.to_string_lossy().to_string();
    sqlx::query("VACUUM INTO ?")
        .bind(path)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|error| format!("snapshot database: {error}"))
}

fn archive_backup(
    dir: &Path,
    staging: &Path,
    backup_name: String,
    locale: chaos_i18n::Locale,
) -> Result<Json<CreateBackupResponse>, ApiError> {
    let mut members: Vec<String> = Vec::new();
    if staging.join("chaos.db").is_file() {
        members.push("chaos.db".into());
    }
    if staging.join("config.dae").is_file() {
        members.push("config.dae".into());
    }
    if members.is_empty() {
        let _ = std::fs::remove_dir_all(staging);
        return Err(ApiError::bad_request("nothing_to_backup", locale));
    }

    let backup_path = dir.join(&backup_name);
    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&backup_path)
        .arg("-C")
        .arg(staging)
        .args(&members)
        .status()
        .map_err(|e| ApiError::internal_logged(locale, format!("run tar: {e}")))?;

    let _ = std::fs::remove_dir_all(staging);

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

    // Stream the archive instead of reading it into memory: an archive can be
    // tens of megabytes, and buffering it doubles peak memory for a transfer
    // that is bound by the network anyway.
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| ApiError::internal_logged(locale, format!("open backup: {e}")))?;
    let body = Body::from_stream(async_stream::stream! {
        let mut file = file;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match file.read(&mut buf).await {
                Ok(0) => break,
                Ok(read) => yield Ok::<Bytes, std::io::Error>(Bytes::copy_from_slice(&buf[..read])),
                Err(error) => {
                    yield Err(error);
                    break;
                }
            }
        }
    });

    Response::builder()
        .header(header::CONTENT_TYPE, "application/gzip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{name}\""),
        )
        .body(body)
        .map_err(|e| ApiError::internal_logged(locale, format!("build download response: {e}")))
}

/// Restore DB and/or dae config from a backup archive.
/// Caller should restart chaos-api after restore so SQLite pool reloads the file.
async fn restore_backup(
    _admin: AdminUser,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<RestoreRequest>,
) -> Result<Json<RestoreResponse>, ApiError> {
    // `tar -xzf` and the file replacements below are blocking, so run them off
    // the async worker threads.
    tokio::task::spawn_blocking(move || restore_backup_blocking(body.name, locale))
        .await
        .map_err(|error| {
            ApiError::internal_logged(locale, format!("restore task failed: {error}"))
        })?
}

fn restore_backup_blocking(
    name: String,
    locale: chaos_i18n::Locale,
) -> Result<Json<RestoreResponse>, ApiError> {
    if !is_safe_backup_name(&name) {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let archive = backup_dir().join(&name);
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
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600));
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

#[cfg(test)]
mod tests {
    use super::*;
    use chaos_store::migrate;
    use sqlx::sqlite::SqliteConnectOptions;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Open a pool on a real file, bypassing `sqlite:` URL parsing.
    async fn open_file(path: &Path) -> SqlitePool {
        SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await
        .unwrap()
    }

    /// The archive must hold every committed transaction, including rows that a
    /// byte copy of the main database file would miss while they sit in the
    /// write-ahead log.
    #[tokio::test]
    async fn snapshot_database_captures_committed_rows() {
        let dir =
            std::env::temp_dir().join(format!("chaos-backup-snapshot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let pool = open_file(&dir.join("live.db")).await;
        migrate(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, created_at, role) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("u1")
        .bind("admin")
        .bind("test-hash")
        .bind("now")
        .bind("admin")
        .execute(&pool)
        .await
        .unwrap();

        let snapshot = dir.join("snapshot.db");
        snapshot_database(&pool, &snapshot).await.unwrap();

        // The snapshot stands alone: it needs no `-wal` sidecar sitting next to it.
        let copy = open_file(&snapshot).await;
        let (users,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&copy)
            .await
            .unwrap();
        assert_eq!(users, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// End-to-end `POST /api/v1/backups`: the handler snapshots the live
    /// database, archives it, and reports the result. The member names inside
    /// the archive must stay relative, because that is what `restore_backup`
    /// extracts.
    #[tokio::test]
    async fn create_backup_archives_the_live_database() {
        // The paths below come from process-global environment variables, so
        // this test must not run next to another one that changes them. The lock
        // is async-aware because the request under test is awaited while held.
        static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let _guard = ENV_LOCK.lock().await;

        let dir = std::env::temp_dir().join(format!("chaos-backup-create-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let backups = dir.join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        let db = dir.join("chaos.db");

        let pool = open_file(&db).await;
        migrate(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, created_at, role) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("u1")
        .bind("admin")
        .bind("test-hash")
        .bind("now")
        .bind("admin")
        .execute(&pool)
        .await
        .unwrap();

        let state = AppState::new(pool, "test-secret-key-for-jwt-hs256".to_string());
        let token = crate::auth::issue_token("u1", "admin", &state.jwt_secret).unwrap();
        let app = backup_router().with_state(state);

        std::env::set_var("CHAOS_BACKUP_DIR", &backups);
        std::env::set_var("CHAOS_DATABASE_URL", format!("sqlite:{}", db.display()));
        std::env::set_var("CHAOS_DAE_WORK_DIR", dir.join("dae"));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/backups")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        std::env::remove_var("CHAOS_BACKUP_DIR");
        std::env::remove_var("CHAOS_DATABASE_URL");
        std::env::remove_var("CHAOS_DAE_WORK_DIR");

        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(status, StatusCode::OK, "body: {body}");

        let name = body["backup"]["name"].as_str().expect("backup name");
        assert!(
            body["backup"]["bytes"].as_u64().unwrap_or(0) > 0,
            "archive is empty: {body}"
        );
        let archive = backups.join(name);
        assert!(archive.is_file(), "archive missing: {}", archive.display());

        let listing = std::process::Command::new("tar")
            .arg("-tzf")
            .arg(&archive)
            .output()
            .expect("run tar");
        let listing = String::from_utf8_lossy(&listing.stdout);
        assert!(
            listing.lines().any(|line| line == "chaos.db"),
            "archive members: {listing}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
