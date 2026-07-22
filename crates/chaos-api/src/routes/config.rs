//! Config import/export: download current dae config or import an existing one.

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use chaos_core::config_render::{
    render_dae_config_with_network, ConfigPlane, DnsRuleForConfig, DnsUpstreamForConfig,
    GroupForConfig, GroupMemberForConfig, NodeForConfig, RoutingRuleForConfig,
};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::routes::network::load_network_config;
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

/// Export the current rendered dae configuration.
async fn export_config(
    _user: AuthUser,
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

    // Build a minimal config plane from published orchestration or defaults
    let plane = ConfigPlane::default();
    let network = load_network_config(&state).await?;
    let content = render_dae_config_with_network(&for_config, &plane, &network);

    // Return as downloadable file
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

/// Import a dae configuration file (placeholder - full parsing not implemented).
async fn import_config(
    _user: AuthUser,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<ImportConfigRequest>,
) -> Result<Json<ImportConfigResponse>, ApiError> {
    // Basic validation: check if it looks like a dae config
    let content = body.content.trim();
    if content.is_empty() {
        return Err(ApiError::bad_request("empty_config", locale));
    }

    // Check for basic dae config structure
    let has_global = content.contains("global {") || content.contains("global{");
    let has_node = content.contains("node {") || content.contains("node{");

    if !has_global && !has_node {
        return Err(ApiError::bad_request("invalid_dae_config", locale));
    }

    // Full parsing would extract nodes, groups, routing rules, DNS settings
    // For now, return a message that import is partially supported
    Ok(Json(ImportConfigResponse {
        ok: true,
        nodes_imported: 0,
        message: "Config validated. Full node extraction requires manual import via share links.".to_string(),
    }))
}

pub fn config_router() -> Router<AppState> {
    Router::new()
        .route("/config/export", get(export_config))
        .route("/config/import", post(import_config))
}
