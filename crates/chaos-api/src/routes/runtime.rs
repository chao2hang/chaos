//! Runtime: dae status, apply (render+reload), stop.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chaos_core::config_render::{
    render_dae_config, ConfigPlane, DnsRuleForConfig, DnsUpstreamForConfig, GroupForConfig,
    NodeForConfig, RoutingRuleForConfig,
};
use chaos_dae::{dae_bin_ok, resolve_dae_bin, DaeManager};
use chaos_i18n::Locale;
use serde::Serialize;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
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

fn manager_or_missing(locale: Locale) -> Result<DaeManager, ApiError> {
    let Some(bin) = resolve_dae_bin() else {
        return Err(ApiError::coded(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "dae_binary_missing",
            locale,
        ));
    };
    if !dae_bin_ok(&bin) {
        return Err(ApiError::coded(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "dae_binary_missing",
            locale,
        ));
    }
    Ok(DaeManager::new(bin, dae_work_dir()))
}

async fn get_runtime(
    _user: AuthUser,
    _state: State<AppState>,
) -> Result<Json<RuntimeStatus>, ApiError> {
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
    RequestLocale(locale): RequestLocale,
) -> Result<Json<ApplyResponse>, ApiError> {
    let mgr = manager_or_missing(locale)?;
    let nodes = chaos_store::nodes::list_nodes(&state.pool).await?;
    let for_config: Vec<NodeForConfig> = nodes
        .iter()
        .map(|n| NodeForConfig {
            id: n.id.clone(),
            name: n.name.clone(),
            link: n.link.clone(),
        })
        .collect();
    let plane = load_config_plane(&state).await?;
    let content = render_dae_config(&for_config, &plane);
    let config_path = mgr
        .write_config(&content)
        .await
        .map_err(|e| ApiError::internal_logged(locale, format!("write config: {e}")))?;
    mgr.reload().await.map_err(|e| map_dae_reload_error(locale, &e.to_string()))?;
    Ok(Json(ApplyResponse {
        ok: true,
        running: mgr.is_running(),
        config_path: config_path.display().to_string(),
        nodes: for_config.len(),
    }))
}

async fn stop_runtime(
    _user: AuthUser,
    RequestLocale(locale): RequestLocale,
) -> Result<Json<RuntimeStatus>, ApiError> {
    if let Ok(mgr) = manager_or_missing(locale) {
        mgr.stop()
            .await
            .map_err(|e| ApiError::internal_logged(locale, format!("stop dae: {e}")))?;
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

fn map_dae_reload_error(locale: Locale, msg: &str) -> ApiError {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("missing") || lower.contains("not a file") {
        return ApiError::coded(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "dae_binary_missing",
            locale,
        );
    }
    if lower.contains("permission")
        || lower.contains("operation not permitted")
        || lower.contains("capabilities")
        || lower.contains("cap_net")
        || lower.contains("requires the file is not writable")
        || lower.contains("too open")
    {
        // Prefer concrete dae diagnostics over a generic localized line when present.
        let detail = truncate_detail(msg, 800);
        return ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "dae_permission_denied",
            detail,
        );
    }
    // Surface start/log excerpt so the UI can show why dae died.
    let detail = truncate_detail(msg, 1200);
    ApiError::new(
        axum::http::StatusCode::BAD_GATEWAY,
        "dae_start_failed",
        detail,
    )
}

fn truncate_detail(msg: &str, max: usize) -> String {
    let cleaned = msg.trim();
    if cleaned.chars().count() <= max {
        return cleaned.to_string();
    }
    let mut out: String = cleaned.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

async fn load_config_plane(state: &AppState) -> Result<ConfigPlane, ApiError> {
    let groups = chaos_store::list_groups(&state.pool).await?;
    let routing_rules = chaos_store::list_routing_rules(&state.pool).await?;
    let routing_fallback = chaos_store::get_meta(&state.pool, chaos_store::META_ROUTING_FALLBACK)
        .await?
        .unwrap_or_else(|| "proxy".into());
    let dns_upstreams = chaos_store::list_dns_upstreams(&state.pool).await?;
    let dns_rules = chaos_store::list_dns_rules(&state.pool).await?;
    let dns_fallback = chaos_store::get_meta(&state.pool, chaos_store::META_DNS_FALLBACK)
        .await?
        .unwrap_or_else(|| "alidns".into());

    Ok(ConfigPlane {
        groups: groups
            .into_iter()
            .map(|g| GroupForConfig {
                name: g.name,
                policy: g.policy,
                filter_tag: g.filter_tag,
            })
            .collect(),
        routing_rules: routing_rules
            .into_iter()
            .map(|r| RoutingRuleForConfig {
                expression: r.expression,
                outbound: r.outbound,
                enabled: r.enabled != 0,
            })
            .collect(),
        routing_fallback,
        dns_upstreams: dns_upstreams
            .into_iter()
            .map(|u| DnsUpstreamForConfig {
                name: u.name,
                address: u.address,
            })
            .collect(),
        dns_rules: dns_rules
            .into_iter()
            .map(|r| DnsRuleForConfig {
                expression: r.expression,
                upstream: r.upstream,
                enabled: r.enabled != 0,
            })
            .collect(),
        dns_fallback,
    })
}

pub fn runtime_router() -> Router<AppState> {
    Router::new()
        .route("/runtime", get(get_runtime))
        .route("/runtime/apply", post(apply_runtime))
        .route("/runtime/stop", post(stop_runtime))
}
