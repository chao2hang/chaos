//! Runtime: dae status, apply (render+reload), stop.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chaos_core::config_render::{
    render_dae_config, ConfigPlane, DnsRuleForConfig, DnsUpstreamForConfig, GroupForConfig,
    GroupMemberForConfig, NodeForConfig, RoutingRuleForConfig,
};
use chaos_core::orchestration::OrchestrationDocument;
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
    pub needs_republish: bool,
    pub data_plane: &'static str,
    pub data_plane_ready: bool,
}

#[derive(Debug, Serialize)]
pub struct ApplyResponse {
    pub ok: bool,
    pub running: bool,
    pub config_path: String,
    pub nodes: usize,
    pub needs_republish: bool,
    pub data_plane: &'static str,
}

fn dae_work_dir() -> std::path::PathBuf {
    std::env::var("CHAOS_DAE_WORK_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("./data/dae"))
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
    }))
}

async fn apply_runtime(
    _user: AuthUser,
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
    let content = render_dae_config(&for_config, &plane);
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
    if let Err(error) = mgr.reload().await {
        // A failed cold restart must not leave the previous daemon/config dead.
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
    Ok(ApplyResponse {
        ok: true,
        running: mgr.is_running(),
        config_path: config_path.display().to_string(),
        nodes: for_config.len(),
        needs_republish: false,
        data_plane: chaos_dae::platform_backend().status().kind,
    })
}

async fn stop_runtime(
    _user: AuthUser,
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

async fn load_config_plane(state: &AppState, locale: Locale) -> Result<ConfigPlane, ApiError> {
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

pub fn runtime_router() -> Router<AppState> {
    Router::new()
        .route("/runtime", get(get_runtime))
        .route("/runtime/apply", post(apply_runtime))
        .route("/runtime/stop", post(stop_runtime))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn state() -> AppState {
        let pool = chaos_store::connect("sqlite::memory:").await.unwrap();
        chaos_store::migrate(&pool).await.unwrap();
        AppState::new(pool, "test-secret-key-for-jwt-hs256".to_string())
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
