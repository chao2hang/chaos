//! Runtime: dae status, apply (render+reload), stop.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chaos_core::config_render::{render_minimal_dae_config, NodeForConfig};
use chaos_dae::{dae_bin_ok, resolve_dae_bin, DaeManager};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct RuntimeStatus {
    pub running: bool,
    pub dae_binary: Option<String>,
    pub dae_binary_ok: bool,
    pub work_dir: String,
    pub config_exists: bool,
}

#[derive(Debug, Serialize)]
pub struct ApplyResponse {
    pub ok: bool,
    pub running: bool,
    pub config_path: String,
    pub nodes: usize,
}

fn dae_work_dir() -> std::path::PathBuf {
    std::env::var("CHAOS_DAE_WORK_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("./data/dae"))
}

fn manager_or_missing() -> Result<DaeManager, ApiError> {
    let Some(bin) = resolve_dae_bin() else {
        return Err(ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "dae_binary_missing",
            "dae binary not found; set CHAOS_DAE_BIN or run scripts/fetch-dae.sh",
        ));
    };
    if !dae_bin_ok(&bin) {
        return Err(ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "dae_binary_missing",
            format!("dae binary path is not a file: {}", bin.display()),
        ));
    }
    Ok(DaeManager::new(bin, dae_work_dir()))
}

async fn get_runtime(_user: AuthUser, _state: State<AppState>) -> Result<Json<RuntimeStatus>, ApiError> {
    let bin = resolve_dae_bin();
    let bin_ok = bin.as_ref().map(|p| dae_bin_ok(p)).unwrap_or(false);
    let work_dir = dae_work_dir();
    let running = if let Some(ref b) = bin {
        if bin_ok {
            DaeManager::new(b.clone(), work_dir.clone()).is_running()
        } else {
            false
        }
    } else {
        false
    };
    let config_exists = work_dir.join("config.dae").is_file();
    Ok(Json(RuntimeStatus {
        running,
        dae_binary: bin.map(|p| p.display().to_string()),
        dae_binary_ok: bin_ok,
        work_dir: work_dir.display().to_string(),
        config_exists,
    }))
}

async fn apply_runtime(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApplyResponse>, ApiError> {
    let mgr = manager_or_missing()?;
    let nodes = chaos_store::nodes::list_nodes(&state.pool).await?;
    let for_config: Vec<NodeForConfig> = nodes
        .iter()
        .map(|n| NodeForConfig {
            id: n.id.clone(),
            name: n.name.clone(),
            link: n.link.clone(),
        })
        .collect();
    let content = render_minimal_dae_config(&for_config);
    let config_path = mgr
        .write_config(&content)
        .await
        .map_err(|e| ApiError::internal(format!("write config: {e}")))?;
    mgr.reload()
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("missing") || msg.contains("not a file") {
                ApiError::new(
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "dae_binary_missing",
                    msg,
                )
            } else if msg.contains("Permission") || msg.contains("permission") {
                ApiError::new(
                    axum::http::StatusCode::FORBIDDEN,
                    "dae_permission_denied",
                    msg,
                )
            } else {
                ApiError::internal(format!("dae reload: {msg}"))
            }
        })?;
    Ok(Json(ApplyResponse {
        ok: true,
        running: mgr.is_running(),
        config_path: config_path.display().to_string(),
        nodes: for_config.len(),
    }))
}

async fn stop_runtime(_user: AuthUser) -> Result<Json<RuntimeStatus>, ApiError> {
    if let Ok(mgr) = manager_or_missing() {
        mgr.stop()
            .await
            .map_err(|e| ApiError::internal(format!("stop dae: {e}")))?;
    }
    let bin = resolve_dae_bin();
    let bin_ok = bin.as_ref().map(|p| dae_bin_ok(p)).unwrap_or(false);
    let work_dir = dae_work_dir();
    Ok(Json(RuntimeStatus {
        running: false,
        dae_binary: bin.map(|p| p.display().to_string()),
        dae_binary_ok: bin_ok,
        work_dir: work_dir.display().to_string(),
        config_exists: work_dir.join("config.dae").is_file(),
    }))
}

pub fn runtime_router() -> Router<AppState> {
    Router::new()
        .route("/runtime", get(get_runtime))
        .route("/runtime/apply", post(apply_runtime))
        .route("/runtime/stop", post(stop_runtime))
}
