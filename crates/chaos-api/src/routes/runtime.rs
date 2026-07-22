//! Runtime: dae status, apply (render+reload), stop, logs.

use std::convert::Infallible;
use std::io::SeekFrom;

use axum::extract::{Query, State};
use axum::response::sse::{Event, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use chaos_core::config_render::{
    render_dae_config_with_network, ConfigPlane, DnsRuleForConfig, DnsUpstreamForConfig,
    GroupForConfig, GroupMemberForConfig, NodeForConfig, RoutingRuleForConfig,
};
use chaos_core::orchestration::{migrate_orchestration_document, OrchestrationDocument};
use chaos_dae::{dae_bin_ok, resolve_dae_bin, DaeManager, ReloadOutcome};
use chaos_i18n::Locale;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};

use crate::auth::{AdminUser, AuthUser};
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::routes::network::load_network_config;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct RuntimeStatus {
    pub running: bool,
    pub dae_binary: Option<String>,
    pub dae_binary_ok: bool,
    pub work_dir: String,
    pub config_exists: bool,
    pub needs_republish: bool,
    pub data_plane: &'static str,
    pub data_plane_ready: bool,
    pub geoip_data: GeoIpDataStatus,
    pub geosite_data: GeoIpDataStatus,
}

#[derive(Debug, Serialize)]
pub struct GeoIpDataStatus {
    pub path: String,
    pub exists: bool,
    pub bytes: u64,
}

const GEOIP_DATA_URL: &str = "https://github.com/v2fly/geoip/releases/latest/download/geoip.dat";
const GEOSITE_DATA_URL: &str =
    "https://github.com/v2fly/domain-list-community/releases/latest/download/dlc.dat";
const MAX_GEO_DATA_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct ApplyResponse {
    pub ok: bool,
    pub running: bool,
    pub config_path: String,
    pub nodes: usize,
    pub needs_republish: bool,
    pub data_plane: &'static str,
    /// How the config was applied: "hot" (zero-downtime reload), "cold" (restart),
    /// or "cold_start" (dae was not running).
    pub reload_method: &'static str,
}

fn dae_work_dir() -> std::path::PathBuf {
    std::env::var("CHAOS_DAE_WORK_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("./data/dae"))
}

/// Start the last successfully rendered dae configuration after a service
/// restart. This is opt-in so development runs never unexpectedly alter host
/// networking; the packaged systemd unit enables it explicitly.
pub async fn restore_persisted_runtime() {
    if !env_flag("CHAOS_AUTOSTART_DAE") {
        return;
    }
    let backend = chaos_dae::platform_backend().status();
    if backend.kind != "linux-dae" || !backend.ready {
        tracing::warn!(reason = backend.reason, "dae autostart skipped");
        return;
    }

    let manager = manager_for_status_or_stop();
    if manager.is_running() {
        tracing::info!("dae is already running; keeping existing runtime");
        return;
    }
    if !manager.config_path().is_file() {
        tracing::info!("no saved dae configuration to restore");
        return;
    }

    if let Err(error) = manager.validate_config().await {
        tracing::error!(error = %error, "saved dae configuration is invalid; autostart skipped");
        return;
    }
    match manager.reload().await {
        Ok(()) => tracing::info!("restored dae runtime from saved configuration"),
        Err(error) => tracing::error!(error = %error, "failed to restore dae runtime"),
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn manager_or_missing(locale: Locale) -> Result<DaeManager, ApiError> {
    let backend = chaos_dae::platform_backend().status();
    if !backend.ready {
        let code = if backend.kind == "windows-wintun-engine" {
            "windows_data_plane_unavailable"
        } else {
            "dae_binary_missing"
        };
        return Err(ApiError::coded(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            code,
            locale,
        ));
    }
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

fn manager_for_status_or_stop() -> DaeManager {
    let bin = resolve_dae_bin()
        .unwrap_or_else(|| std::path::PathBuf::from("third_party/dae/current/dae"));
    DaeManager::new(bin, dae_work_dir())
}

async fn get_runtime(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<RuntimeStatus>, ApiError> {
    let bin = resolve_dae_bin();
    let work_dir = dae_work_dir();
    let data_plane = chaos_dae::platform_backend().status();
    let bin = (data_plane.kind == "linux-dae").then_some(bin).flatten();
    let bin_ok = bin.as_ref().map(|p| dae_bin_ok(p)).unwrap_or(false);
    let running = data_plane.kind == "linux-dae" && manager_for_status_or_stop().is_running();
    let config_exists = work_dir.join("config.dae").is_file();
    let geoip_data = geo_data_status(&work_dir, "geoip.dat");
    let geosite_data = geo_data_status(&work_dir, "geosite.dat");
    let needs_republish = orchestration_needs_republish(&state).await?;
    Ok(Json(RuntimeStatus {
        running,
        dae_binary: bin.map(|p| p.display().to_string()),
        dae_binary_ok: bin_ok,
        work_dir: work_dir.display().to_string(),
        config_exists,
        needs_republish,
        data_plane: data_plane.kind,
        data_plane_ready: data_plane.ready,
        geoip_data,
        geosite_data,
    }))
}

fn geo_data_status(work_dir: &std::path::Path, filename: &str) -> GeoIpDataStatus {
    let path = work_dir.join(filename);
    let metadata = std::fs::metadata(&path).ok();
    GeoIpDataStatus {
        path: path.display().to_string(),
        exists: metadata.is_some(),
        bytes: metadata.map(|metadata| metadata.len()).unwrap_or_default(),
    }
}

async fn download_geo_dataset(
    url: &str,
    too_large_code: &'static str,
    locale: Locale,
) -> Result<Vec<u8>, ApiError> {
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| {
            ApiError::internal_logged(locale, format!("download geo data from {url}: {error}"))
        })?
        .error_for_status()
        .map_err(|error| {
            ApiError::internal_logged(locale, format!("download geo data status {url}: {error}"))
        })?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_GEO_DATA_BYTES)
    {
        return Err(ApiError::bad_request(too_large_code, locale));
    }
    let data = response.bytes().await.map_err(|error| {
        ApiError::internal_logged(locale, format!("read geo data from {url}: {error}"))
    })?;
    if u64::try_from(data.len()).unwrap_or(u64::MAX) > MAX_GEO_DATA_BYTES {
        return Err(ApiError::bad_request(too_large_code, locale));
    }
    Ok(data.to_vec())
}

async fn update_geoip_data(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
) -> Result<Json<GeoIpDataStatus>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    let manager = manager_for_status_or_stop();
    let data = download_geo_dataset(GEOIP_DATA_URL, "geoip_data_too_large", locale).await?;
    manager
        .write_geoip_data(&data)
        .await
        .map_err(|error| ApiError::internal_logged(locale, format!("write geoip data: {error}")))?;
    Ok(Json(geo_data_status(&manager.work_dir, "geoip.dat")))
}

async fn update_geosite_data(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
) -> Result<Json<GeoIpDataStatus>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    let manager = manager_for_status_or_stop();
    let data = download_geo_dataset(GEOSITE_DATA_URL, "geosite_data_too_large", locale).await?;
    manager
        .write_geosite_data(&data)
        .await
        .map_err(|error| ApiError::internal_logged(locale, format!("write geosite data: {error}")))?;
    Ok(Json(geo_data_status(&manager.work_dir, "geosite.dat")))
}

async fn apply_runtime(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
) -> Result<Json<ApplyResponse>, ApiError> {
    Ok(Json(apply_current_config(&state, locale).await?))
}

pub(crate) async fn apply_current_config(
    state: &AppState,
    locale: Locale,
) -> Result<ApplyResponse, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    apply_current_config_locked(state, locale).await
}

/// Apply while the caller already owns `runtime_lock` (publication path).
pub(crate) async fn apply_current_config_locked(
    state: &AppState,
    locale: Locale,
) -> Result<ApplyResponse, ApiError> {
    if orchestration_needs_republish(state).await? {
        return Err(ApiError::conflict("orchestration_needs_republish", locale));
    }
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
    let plane = load_config_plane(state, locale).await?;
    let network = load_network_config(state).await?;
    let content = render_dae_config_with_network(&for_config, &plane, &network);
    let previous_config = tokio::fs::read_to_string(mgr.config_path()).await.ok();
    let config_path = mgr
        .write_config(&content)
        .await
        .map_err(|e| ApiError::internal_logged(locale, format!("write config: {e}")))?;
    if let Err(error) = mgr.validate_config().await {
        if let Some(previous) = previous_config.clone() {
            let _ = mgr.write_config(&previous).await;
        } else {
            let _ = tokio::fs::remove_file(mgr.config_path()).await;
        }
        return Err(map_dae_reload_error(locale, &error.to_string()));
    }
    // Prefer zero-downtime hot reload; fall back to cold restart on failure.
    let outcome = match mgr.hot_reload().await {
        Ok(outcome) => outcome,
        Err(error) => {
            // A failed reload must not leave the previous daemon/config dead.
            if let Some(previous) = previous_config {
                if let Err(restore_error) = mgr.write_config(&previous).await {
                    tracing::error!(error = %restore_error, "failed to restore previous dae config");
                } else if let Err(restore_error) = mgr.reload().await {
                    tracing::error!(error = %restore_error, "failed to restart dae with restored config");
                }
            } else {
                let _ = tokio::fs::remove_file(mgr.config_path()).await;
            }
            return Err(map_dae_reload_error(locale, &error.to_string()));
        }
    };
    let reload_method = match outcome {
        ReloadOutcome::Hot => "hot",
        ReloadOutcome::ColdStart => "cold_start",
        ReloadOutcome::ColdFallback => "cold",
    };
    Ok(ApplyResponse {
        ok: true,
        running: mgr.is_running(),
        config_path: config_path.display().to_string(),
        nodes: for_config.len(),
        needs_republish: false,
        data_plane: chaos_dae::platform_backend().status().kind,
        reload_method,
    })
}

async fn stop_runtime(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
) -> Result<Json<RuntimeStatus>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    let data_plane = chaos_dae::platform_backend().status();
    if data_plane.kind != "linux-dae" {
        return Err(ApiError::coded(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "windows_data_plane_unavailable",
            locale,
        ));
    }
    manager_for_status_or_stop()
        .stop()
        .await
        .map_err(|e| ApiError::internal_logged(locale, format!("stop dae: {e}")))?;
    let bin = resolve_dae_bin();
    let work_dir = dae_work_dir();
    let needs_republish = orchestration_needs_republish(&state).await?;
    let bin = (data_plane.kind == "linux-dae").then_some(bin).flatten();
    let bin_ok = bin.as_ref().map(|p| dae_bin_ok(p)).unwrap_or(false);
    Ok(Json(RuntimeStatus {
        running: false,
        dae_binary: bin.map(|p| p.display().to_string()),
        dae_binary_ok: bin_ok,
        work_dir: work_dir.display().to_string(),
        config_exists: work_dir.join("config.dae").is_file(),
        needs_republish,
        data_plane: data_plane.kind,
        data_plane_ready: data_plane.ready,
        geoip_data: geo_data_status(&work_dir, "geoip.dat"),
        geosite_data: geo_data_status(&work_dir, "geosite.dat"),
    }))
}

async fn orchestration_needs_republish(state: &AppState) -> Result<bool, ApiError> {
    Ok(
        chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_NEEDS_REPUBLISH)
            .await?
            .as_deref()
            == Some("true"),
    )
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
        let detail = truncate_detail(&redact_runtime_detail(msg), 800);
        return ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "dae_permission_denied",
            detail,
        );
    }
    // Surface start/log excerpt so the UI can show why dae died.
    let detail = truncate_detail(&redact_runtime_detail(msg), 1200);
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

fn redact_runtime_detail(message: &str) -> String {
    const PROXY_SCHEMES: [&str; 8] = [
        "ss://",
        "ssr://",
        "vmess://",
        "vless://",
        "trojan://",
        "hysteria://",
        "hysteria2://",
        "tuic://",
    ];
    message
        .lines()
        .map(|line| {
            PROXY_SCHEMES
                .iter()
                .filter_map(|scheme| line.find(scheme))
                .min()
                .map_or_else(
                    || line.to_string(),
                    |index| format!("{}[redacted proxy URL]", &line[..index]),
                )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) async fn load_config_plane(
    state: &AppState,
    locale: Locale,
) -> Result<ConfigPlane, ApiError> {
    let (groups, routing_rules, routing_fallback) =
        load_published_or_legacy_routing(state, locale).await?;
    let dns_upstreams = chaos_store::list_dns_upstreams(&state.pool).await?;
    let dns_rules = chaos_store::list_dns_rules(&state.pool).await?;
    let dns_fallback = chaos_store::get_meta(&state.pool, chaos_store::META_DNS_FALLBACK)
        .await?
        .unwrap_or_else(|| "alidns".into());

    Ok(ConfigPlane {
        groups,
        routing_rules,
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

/// Compile the last published V2 graph on every apply. Drafts and mutable
/// legacy tables are not active runtime configuration after V2 is initialized.
async fn load_published_or_legacy_routing(
    state: &AppState,
    locale: Locale,
) -> Result<(Vec<GroupForConfig>, Vec<RoutingRuleForConfig>, String), ApiError> {
    let marker = chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_V2_INITIALIZED)
        .await?
        .as_deref()
        == Some("true");
    let plan = if let Some(raw) =
        chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_PLAN).await?
    {
        serde_json::from_str::<chaos_store::PublishedOrchestrationPlan>(&raw).map_err(|error| {
            ApiError::internal_logged(locale, format!("invalid published plan: {error}"))
        })?
    } else if marker {
        // One-time recovery for V2 databases written before META_PLAN existed.
        let raw = chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_FLOW)
            .await?
            .ok_or_else(|| {
                ApiError::coded(
                    axum::http::StatusCode::CONFLICT,
                    "orchestration_not_published",
                    locale,
                )
            })?;
        recover_legacy_v2_plan(state, &raw, locale).await?
    } else {
        return load_legacy_config_plane(state).await;
    };

    let document: OrchestrationDocument =
        serde_json::from_str(&plan.document).map_err(|error| {
            ApiError::internal_logged(locale, format!("invalid published graph: {error}"))
        })?;
    let document = migrate_orchestration_document(document);
    let compiled = document.compile().map_err(|report| {
        tracing::error!(issues = ?report.issues, "published orchestration no longer compiles");
        ApiError::bad_request("orchestration_invalid", locale)
    })?;
    let node_ids: std::collections::HashSet<String> = chaos_store::list_nodes(&state.pool)
        .await?
        .into_iter()
        .map(|node| node.id)
        .collect();
    let mut groups = Vec::with_capacity(plan.groups.len());
    for group in plan.groups {
        if group.members.is_empty() || group.members.iter().any(|(id, _)| !node_ids.contains(id)) {
            return Err(ApiError::coded(
                axum::http::StatusCode::CONFLICT,
                "orchestration_stale",
                locale,
            ));
        }
        groups.push(GroupForConfig {
            name: group.name,
            policy: group.policy,
            filter_tag: None,
            members: group
                .members
                .into_iter()
                .map(|(node_id, weight)| GroupMemberForConfig {
                    node_id,
                    weight: u32::try_from(weight.clamp(1, 99)).unwrap_or(1),
                })
                .collect(),
        });
    }
    let rules = compiled
        .conditions
        .into_iter()
        .map(|route| RoutingRuleForConfig {
            expression: route.condition,
            outbound: route.outbound,
            enabled: true,
        })
        .collect();
    Ok((groups, rules, compiled.fallback))
}

async fn load_legacy_config_plane(
    state: &AppState,
) -> Result<(Vec<GroupForConfig>, Vec<RoutingRuleForConfig>, String), ApiError> {
    let groups = chaos_store::list_groups(&state.pool).await?;
    let all_members = chaos_store::list_all_group_members(&state.pool).await?;
    let mut members_by_group: std::collections::HashMap<String, Vec<GroupMemberForConfig>> =
        std::collections::HashMap::new();
    for member in all_members {
        members_by_group
            .entry(member.group_id)
            .or_default()
            .push(GroupMemberForConfig {
                node_id: member.node_id,
                weight: u32::try_from(member.weight.max(1)).unwrap_or(1),
            });
    }
    let groups = groups
        .into_iter()
        .map(|group| GroupForConfig {
            members: members_by_group.remove(&group.id).unwrap_or_default(),
            name: group.name,
            policy: group.policy,
            filter_tag: group.filter_tag,
        })
        .collect();
    let rules = chaos_store::list_routing_rules(&state.pool)
        .await?
        .into_iter()
        .map(|rule| RoutingRuleForConfig {
            expression: rule.expression,
            outbound: rule.outbound,
            enabled: rule.enabled != 0,
        })
        .collect();
    let fallback = chaos_store::get_meta(&state.pool, chaos_store::META_ROUTING_FALLBACK)
        .await?
        .unwrap_or_else(|| "proxy".into());
    Ok((groups, rules, fallback))
}

async fn recover_legacy_v2_plan(
    state: &AppState,
    raw_document: &str,
    locale: Locale,
) -> Result<chaos_store::PublishedOrchestrationPlan, ApiError> {
    let document: OrchestrationDocument = serde_json::from_str(raw_document).map_err(|error| {
        ApiError::internal_logged(locale, format!("invalid published graph: {error}"))
    })?;
    let document = migrate_orchestration_document(document);
    let compiled = document
        .compile()
        .map_err(|_| ApiError::bad_request("orchestration_invalid", locale))?;
    let groups = chaos_store::list_groups(&state.pool).await?;
    let members = chaos_store::list_all_group_members(&state.pool).await?;
    let mut members_by_group: std::collections::HashMap<String, Vec<(String, i64)>> =
        std::collections::HashMap::new();
    for member in members {
        members_by_group
            .entry(member.group_id)
            .or_default()
            .push((member.node_id, member.weight.clamp(1, 99)));
    }
    let group_by_id: std::collections::HashMap<_, _> = groups
        .into_iter()
        .map(|group| (group.id.clone(), group))
        .collect();
    let mut published_groups = Vec::new();
    for node in document
        .nodes
        .iter()
        .filter(|node| node.kind == chaos_core::orchestration::FlowNodeKind::NodeGroup)
    {
        let runtime_id = node.data.runtime_group_id.as_deref().ok_or_else(|| {
            ApiError::coded(
                axum::http::StatusCode::CONFLICT,
                "orchestration_stale",
                locale,
            )
        })?;
        let group = group_by_id.get(runtime_id).ok_or_else(|| {
            ApiError::coded(
                axum::http::StatusCode::CONFLICT,
                "orchestration_stale",
                locale,
            )
        })?;
        let members = members_by_group
            .get(runtime_id)
            .cloned()
            .unwrap_or_default();
        if members.is_empty() {
            return Err(ApiError::coded(
                axum::http::StatusCode::CONFLICT,
                "orchestration_stale",
                locale,
            ));
        }
        published_groups.push(chaos_store::PublishedGroup {
            node_id: node.id.clone(),
            id: runtime_id.to_string(),
            name: node.data.name.trim().to_string(),
            policy: node.data.policy.trim().to_string(),
            members,
        });
        let _ = group;
    }
    let plan = chaos_store::PublishedOrchestrationPlan {
        document: raw_document.to_string(),
        groups: published_groups,
        routing: compiled
            .conditions
            .iter()
            .map(|route| chaos_store::PublishedRoutingRule {
                expression: route.condition.clone(),
                outbound: route.outbound.clone(),
            })
            .collect(),
    };
    let serialized =
        serde_json::to_string(&plan).map_err(|error| ApiError::internal_logged(locale, error))?;
    chaos_store::set_meta(
        &state.pool,
        chaos_store::META_ORCHESTRATION_PLAN,
        &serialized,
    )
    .await?;
    Ok(plan)
}

/// Hot-reload the running dae process without re-rendering config.
///
/// Useful when the config file was already written (e.g. by a profile switch)
/// and only a reload signal is needed.
async fn reload_runtime(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
) -> Result<Json<ApplyResponse>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    let mgr = manager_or_missing(locale)?;
    let config_path = mgr.config_path();
    if !config_path.is_file() {
        return Err(ApiError::bad_request("no_config_to_reload", locale));
    }
    let outcome = mgr
        .hot_reload()
        .await
        .map_err(|e| map_dae_reload_error(locale, &e.to_string()))?;
    let reload_method = match outcome {
        ReloadOutcome::Hot => "hot",
        ReloadOutcome::ColdStart => "cold_start",
        ReloadOutcome::ColdFallback => "cold",
    };
    Ok(Json(ApplyResponse {
        ok: true,
        running: mgr.is_running(),
        config_path: config_path.display().to_string(),
        nodes: 0,
        needs_republish: false,
        data_plane: chaos_dae::platform_backend().status().kind,
        reload_method,
    }))
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Number of trailing lines to return (default 100, max 5000).
    #[serde(default = "default_log_lines")]
    pub lines: usize,
}

fn default_log_lines() -> usize {
    100
}

#[derive(Debug, Serialize)]
pub struct LogsResponse {
    pub path: String,
    pub exists: bool,
    pub lines: Vec<String>,
}

/// Return the last N lines of dae.log (redacted).
async fn get_logs(
    _user: AuthUser,
    Query(params): Query<LogsQuery>,
) -> Result<Json<LogsResponse>, ApiError> {
    let work_dir = dae_work_dir();
    let log_path = work_dir.join("dae.log");
    if !log_path.is_file() {
        return Ok(Json(LogsResponse {
            path: log_path.display().to_string(),
            exists: false,
            lines: vec![],
        }));
    }
    let lines = params.lines.clamp(1, 5000);
    let content = tokio::fs::read_to_string(&log_path)
        .await
        .unwrap_or_default();
    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(lines);
    let tail: Vec<String> = all_lines[start..]
        .iter()
        .map(|l| redact_runtime_detail(l))
        .collect();
    Ok(Json(LogsResponse {
        path: log_path.display().to_string(),
        exists: true,
        lines: tail,
    }))
}

/// SSE stream that tails dae.log in real time.
async fn stream_logs(
    _user: AuthUser,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let work_dir = dae_work_dir();
    let log_path = work_dir.join("dae.log");

    let stream = async_stream::stream! {
        // Open the file (or wait for it to appear).
        let file = loop {
            match tokio::fs::File::open(&log_path).await {
                Ok(f) => break f,
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        };
        let mut reader = BufReader::new(file);
        // Seek to end so we only stream new lines.
        let _ = reader.seek(SeekFrom::End(0)).await;
        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            match reader.read_line(&mut line_buf).await {
                Ok(0) => {
                    // EOF — wait for new data.
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                Ok(_) => {
                    let redacted = redact_runtime_detail(line_buf.trim_end());
                    yield Ok(Event::default().data(redacted));
                }
                Err(_) => break,
            }
        }
    };

    Sse::new(stream)
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct DiagnosticsResponse {
    pub kernel_version: String,
    pub kernel_ok: bool,
    pub ebpf_supported: bool,
    pub cgroup2_mounted: bool,
    pub bpf_fs_mounted: bool,
    pub ip_forward: bool,
    pub interfaces: Vec<String>,
    pub dae_binary_version: Option<String>,
    pub permissions: DiagnosticsPermissions,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsPermissions {
    pub root: bool,
    pub cap_net_admin: bool,
    pub cap_bpf: bool,
}

/// Check system environment for dae requirements.
async fn get_diagnostics(_user: AuthUser) -> Json<DiagnosticsResponse> {
    let kernel_version = read_kernel_version();
    let kernel_ok = check_kernel_version(&kernel_version);
    let ebpf_supported = kernel_ok; // eBPF requires kernel >= 5.17
    let cgroup2_mounted = std::path::Path::new("/sys/fs/cgroup/cgroup.controllers").exists();
    let bpf_fs_mounted = std::path::Path::new("/sys/fs/bpf").is_dir();
    let ip_forward = read_ip_forward();
    let interfaces = list_interfaces();
    let dae_binary_version = read_dae_version();
    let permissions = check_permissions();

    Json(DiagnosticsResponse {
        kernel_version,
        kernel_ok,
        ebpf_supported,
        cgroup2_mounted,
        bpf_fs_mounted,
        ip_forward,
        interfaces,
        dae_binary_version,
        permissions,
    })
}

fn read_kernel_version() -> String {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// dae requires kernel >= 5.17 for eBPF features.
fn check_kernel_version(version: &str) -> bool {
    let parts: Vec<u32> = version
        .split('.')
        .take(2)
        .filter_map(|p| p.parse().ok())
        .collect();
    match parts.as_slice() {
        [major, minor] => *major > 5 || (*major == 5 && *minor >= 17),
        [major] => *major > 5,
        _ => false,
    }
}

fn read_ip_forward() -> bool {
    std::fs::read_to_string("/proc/sys/net/ipv4/ip_forward")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
}

fn list_interfaces() -> Vec<String> {
    std::fs::read_dir("/sys/class/net")
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|name| name != "lo")
                .collect()
        })
        .unwrap_or_default()
}

fn read_dae_version() -> Option<String> {
    let bin = resolve_dae_bin()?;
    let output = std::process::Command::new(&bin)
        .arg("version")
        .output()
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // dae version output is like "dae version v0.2.2" or just "v0.2.2"
        stdout
            .lines()
            .next()
            .map(|l| l.trim().to_string())
    } else {
        None
    }
}

fn check_permissions() -> DiagnosticsPermissions {
    let root = unsafe { libc::geteuid() == 0 };
    // Check capabilities by reading /proc/self/status
    let (cap_net_admin, cap_bpf) = read_capabilities();
    DiagnosticsPermissions {
        root,
        cap_net_admin: root || cap_net_admin,
        cap_bpf: root || cap_bpf,
    }
}

fn read_capabilities() -> (bool, bool) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let mut cap_net_admin = false;
    let mut cap_bpf = false;
    for line in status.lines() {
        if let Some(caps) = line.strip_prefix("CapEff:") {
            if let Ok(cap_hex) = u64::from_str_radix(caps.trim(), 16) {
                // CAP_NET_ADMIN = 12, CAP_BPF = 39
                cap_net_admin = (cap_hex >> 12) & 1 == 1;
                cap_bpf = (cap_hex >> 39) & 1 == 1;
            }
            break;
        }
    }
    (cap_net_admin, cap_bpf)
}

// ---------------------------------------------------------------------------
// Connections (parsed from dae.log)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ConnectionsResponse {
    pub connections: Vec<chaos_core::traffic::ActiveConnection>,
}

/// Return active connections parsed from recent dae.log entries.
async fn get_connections(_user: AuthUser) -> Json<ConnectionsResponse> {
    let work_dir = dae_work_dir();
    let log_path = work_dir.join("dae.log");
    let content = tokio::fs::read_to_string(&log_path).await.unwrap_or_default();

    // Parse recent log lines for connection events
    let mut connections: Vec<chaos_core::traffic::ActiveConnection> = Vec::new();
    let lines: Vec<&str> = content.lines().rev().take(500).collect();

    for line in lines.iter().rev() {
        if let Some(entry) = chaos_core::traffic::parse_dae_log_line(line) {
            if let Some(conn) = entry.connection {
                if conn.action == chaos_core::traffic::ConnectionAction::Open {
                    connections.push(chaos_core::traffic::ActiveConnection {
                        id: format!("{}-{}", conn.source, conn.destination),
                        source: conn.source,
                        destination: conn.destination,
                        outbound: conn.outbound,
                        protocol: conn.protocol,
                        started_at: entry.timestamp,
                        duration_secs: 0,
                        bytes_up: 0,
                        bytes_down: 0,
                    });
                }
            }
        }
    }

    // Limit to most recent 100
    connections.truncate(100);
    Json(ConnectionsResponse { connections })
}

pub fn runtime_router() -> Router<AppState> {
    Router::new()
        .route("/runtime", get(get_runtime))
        .route("/runtime/apply", post(apply_runtime))
        .route("/runtime/reload", post(reload_runtime))
        .route("/runtime/logs", get(get_logs))
        .route("/runtime/logs/stream", get(stream_logs))
        .route("/runtime/diagnostics", get(get_diagnostics))
        .route("/runtime/connections", get(get_connections))
        .route("/runtime/geoip/update", post(update_geoip_data))
        .route("/runtime/geosite/update", post(update_geosite_data))
        .route("/runtime/stop", post(stop_runtime))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::auth::{auth_router, issue_token, issue_token_role};

    async fn state() -> AppState {
        let pool = chaos_store::connect("sqlite::memory:").await.unwrap();
        chaos_store::migrate(&pool).await.unwrap();
        AppState::new(pool, "test-secret-key-for-jwt-hs256".to_string())
    }

    async fn test_app() -> (axum::Router, AppState) {
        let pool = chaos_store::connect("sqlite::memory:").await.unwrap();
        chaos_store::migrate(&pool).await.unwrap();
        for (id, username, role) in [("u1", "admin", "admin"), ("u2", "viewer", "user")] {
            sqlx::query(
                "INSERT INTO users (id, username, password_hash, created_at, role) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(username)
            .bind("test-hash")
            .bind("now")
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();
        }
        let state = AppState::new(pool, "test-secret-key-for-jwt-hs256".to_string());
        let app = axum::Router::new()
            .nest("/api/v1/auth", auth_router())
            .nest("/api/v1", runtime_router())
            .with_state(state.clone());
        (app, state)
    }

    async fn json_body(res: axum::response::Response) -> serde_json::Value {
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn apply_rejects_non_admin() {
        let (app, state) = test_app().await;
        let token = issue_token_role("u2", "viewer", "user", &state.jwt_secret).unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/runtime/apply")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        let body = json_body(res).await;
        assert_eq!(body["error"]["code"], "admin_required");
    }

    #[tokio::test]
    async fn stop_rejects_non_admin() {
        let (app, state) = test_app().await;
        let token = issue_token_role("u2", "viewer", "user", &state.jwt_secret).unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/runtime/stop")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert_eq!(json_body(res).await["error"]["code"], "admin_required");
    }

    #[tokio::test]
    async fn get_runtime_allows_non_admin() {
        let (app, state) = test_app().await;
        let token = issue_token_role("u2", "viewer", "user", &state.jwt_secret).unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/runtime")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let _ = issue_token("u1", "admin", &state.jwt_secret);
    }

    #[tokio::test]
    async fn apply_plane_recompiles_the_published_document() {
        let state = state().await;
        let document = r#"{"version":2,"nodes":[{"id":"direct","type":"builtin","position":{"x":0,"y":0},"data":{"builtin":"direct"}}],"edges":[],"viewport":{"x":0,"y":0,"zoom":1}}"#;
        let plan = chaos_store::PublishedOrchestrationPlan {
            document: document.to_string(),
            groups: vec![],
            // A stale cache must not be consumed by Apply.
            routing: vec![chaos_store::PublishedRoutingRule {
                expression: "domain(suffix: stale.example)".into(),
                outbound: "block".into(),
            }],
        };
        chaos_store::publish_orchestration_v2(&state.pool, &plan)
            .await
            .unwrap();

        let (groups, rules, fallback) = load_published_or_legacy_routing(&state, Locale::En)
            .await
            .unwrap();
        assert!(groups.is_empty());
        assert!(rules.is_empty());
        assert_eq!(fallback, "direct");
    }

    #[tokio::test]
    async fn apply_plane_rejects_missing_published_members() {
        let state = state().await;
        let document = r#"{"version":2,"nodes":[{"id":"group","type":"node_group","position":{"x":0,"y":0},"data":{"name":"proxy","policy":"min_moving_avg","sources":[{"kind":"node","id":"missing","weight":1}],"runtime_group_id":"runtime-group"}},{"id":"direct","type":"builtin","position":{"x":0,"y":0},"data":{"builtin":"direct"}}],"edges":[],"viewport":{"x":0,"y":0,"zoom":1}}"#;
        let plan = chaos_store::PublishedOrchestrationPlan {
            document: document.to_string(),
            groups: vec![chaos_store::PublishedGroup {
                node_id: "group".into(),
                id: "runtime-group".into(),
                name: "proxy".into(),
                policy: "min_moving_avg".into(),
                members: vec![("missing".into(), 1)],
            }],
            routing: vec![],
        };
        chaos_store::publish_orchestration_v2(&state.pool, &plan)
            .await
            .unwrap();

        let error = load_published_or_legacy_routing(&state, Locale::En)
            .await
            .unwrap_err();
        assert_eq!(error.code, "orchestration_stale");
    }
}
