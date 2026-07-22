//! Config import/export: download current dae config or import an existing one.

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use chaos_core::config_render::{render_dae_config_with_network, NodeForConfig};

use crate::auth::AdminUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::routes::network::load_network_config;
use crate::routes::runtime::load_config_plane;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ExportConfigResponse {
    pub config: String,
    pub nodes: usize,
}

#[derive(Debug, Deserialize)]
pub struct ImportConfigRequest {
    /// Raw dae config content to parse and import.
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ImportConfigResponse {
    pub ok: bool,
    pub nodes_imported: usize,
    pub message: String,
}

/// Export the currently published dae configuration plane (same path as apply).
async fn export_config(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
) -> Result<Response, ApiError> {
    let nodes = chaos_store::nodes::list_nodes(&state.pool).await?;
    let for_config: Vec<NodeForConfig> = nodes
        .iter()
        .map(|n| NodeForConfig {
            id: n.id.clone(),
            name: n.name.clone(),
            link: n.link.clone(),
        })
        .collect();

    let plane = load_config_plane(&state, locale).await?;
    let network = load_network_config(&state).await?;
    let content = render_dae_config_with_network(&for_config, &plane, &network);

    let filename = format!(
        "chaos_config_{}.dae",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );

    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                &format!("attachment; filename=\"{filename}\""),
            ),
        ],
        content,
    )
        .into_response())
}

/// Import is not implemented yet — return 501 instead of a false success.
async fn import_config(
    _admin: AdminUser,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<ImportConfigRequest>,
) -> Result<Json<ImportConfigResponse>, ApiError> {
    let content = body.content.trim();
    if content.is_empty() {
        return Err(ApiError::bad_request("empty_config", locale));
    }

    Err(ApiError::not_implemented("not_implemented", locale))
}

pub fn config_router() -> Router<AppState> {
    Router::new()
        .route("/config/export", get(export_config))
        .route("/config/import", post(import_config))
}
