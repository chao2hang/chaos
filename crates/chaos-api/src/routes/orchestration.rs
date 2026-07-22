//! Persisted V2 drag-and-drop orchestration graph and atomic publication.

use std::collections::{HashMap, HashSet};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chaos_core::orchestration::{
    FlowNode, FlowNodeKind, GroupSource, OrchestrationDocument, RouteProbe, RouteSimulation,
    ValidationReport, ORCHESTRATION_VERSION,
};
use chaos_store::{PublishedGroup, PublishedOrchestrationPlan, PublishedRoutingRule};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::routes::runtime::ApplyResponse;
use crate::state::AppState;

const MAX_FLOW_NODES: usize = 500;
const MAX_FLOW_EDGES: usize = 1_500;
const MAX_FLOW_SOURCES: usize = 10_000;

#[derive(Debug, Serialize)]
struct PublishResponse {
    document: OrchestrationDocument,
    applied: ApplyResponse,
}

#[derive(Debug, Serialize)]
struct OrchestrationResponse {
    #[serde(flatten)]
    document: OrchestrationDocument,
    needs_republish: bool,
}

#[derive(Debug)]
struct PublishIssue {
    code: &'static str,
    node_id: String,
}

#[derive(Debug)]
enum PrepareError {
    Validation(ValidationReport),
    Resource(PublishIssue),
    Serialization(serde_json::Error),
}

#[derive(Debug, Default)]
struct SourceCatalog {
    node_ids: HashSet<String>,
    subscription_ids: HashSet<String>,
    nodes_by_subscription: HashMap<String, Vec<String>>,
    group_ids: HashSet<String>,
    members_by_group: HashMap<String, Vec<(String, u32)>>,
}

impl SourceCatalog {
    async fn load(state: &AppState) -> Result<Self, ApiError> {
        let (nodes, subscriptions, groups, members) = tokio::join!(
            chaos_store::list_nodes(&state.pool),
            chaos_store::list_subscriptions(&state.pool),
            chaos_store::list_groups(&state.pool),
            chaos_store::list_all_group_members(&state.pool),
        );
        let nodes = nodes?;
        let subscriptions = subscriptions?;
        let groups = groups?;
        let members = members?;
        let node_ids = nodes.iter().map(|node| node.id.clone()).collect();
        let mut nodes_by_subscription: HashMap<String, Vec<String>> = HashMap::new();
        for node in &nodes {
            if let Some(subscription_id) = &node.subscription_id {
                nodes_by_subscription
                    .entry(subscription_id.clone())
                    .or_default()
                    .push(node.id.clone());
            }
        }
        let mut members_by_group: HashMap<String, Vec<(String, u32)>> = HashMap::new();
        for member in members {
            members_by_group.entry(member.group_id).or_default().push((
                member.node_id,
                u32::try_from(member.weight.clamp(1, 99)).unwrap_or(1),
            ));
        }
        for group in &groups {
            if members_by_group
                .get(&group.id)
                .is_some_and(|members| !members.is_empty())
            {
                continue;
            }
            if let Some(tag) = group
                .filter_tag
                .as_deref()
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
            {
                let tagged = nodes
                    .iter()
                    .filter(|node| node.tag.as_deref() == Some(tag))
                    .map(|node| (node.id.clone(), 1))
                    .collect::<Vec<_>>();
                if !tagged.is_empty() {
                    members_by_group.insert(group.id.clone(), tagged);
                }
            }
        }
        Ok(Self {
            node_ids,
            subscription_ids: subscriptions
                .into_iter()
                .map(|subscription| subscription.id)
                .collect(),
            nodes_by_subscription,
            group_ids: groups.into_iter().map(|group| group.id).collect(),
            members_by_group,
        })
    }
}

pub fn orchestration_router() -> Router<AppState> {
    Router::new()
        .route(
            "/orchestration",
            get(get_orchestration).put(put_orchestration),
        )
        .route("/orchestration/validate", post(validate_document))
        .route("/orchestration/publish", post(publish_document))
        .route("/orchestration/simulate", post(simulate_document))
}

#[derive(Debug, Deserialize)]
struct SimulateRequest {
    document: OrchestrationDocument,
    #[serde(default)]
    probe: RouteProbe,
}

async fn simulate_document(
    _user: AuthUser,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<SimulateRequest>,
) -> Result<Json<RouteSimulation>, ApiError> {
    let document = normalize_document(body.document);
    check_document_limits(&document, locale)?;
    let compiled = document.compile().map_err(|report| {
        tracing::debug!(issues = ?report.issues, "simulate rejected invalid document");
        ApiError::bad_request("orchestration_invalid", locale)
    })?;
    Ok(Json(compiled.simulate(&body.probe)))
}

async fn get_orchestration(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<OrchestrationResponse>, ApiError> {
    // Drafts are intentionally preferred in the editor, while runtime only ever
    // consumes META_ORCHESTRATION_FLOW (the last successful publication).
    let raw = chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_DRAFT)
        .await?
        .or(chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_FLOW).await?)
        .or(chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_FLOW_LEGACY).await?);
    let Some(raw) = raw else {
        return Ok(Json(OrchestrationResponse {
            document: OrchestrationDocument::default(),
            needs_republish: false,
        }));
    };

    let needs_republish =
        chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_NEEDS_REPUBLISH)
            .await?
            .as_deref()
            == Some("true");

    match serde_json::from_str::<OrchestrationDocument>(&raw) {
        Ok(document) => Ok(Json(OrchestrationResponse {
            document: normalize_document(document),
            needs_republish,
        })),
        Err(error) => {
            tracing::error!(error = %error, "invalid persisted orchestration document");
            Ok(Json(OrchestrationResponse {
                document: OrchestrationDocument::default(),
                needs_republish,
            }))
        }
    }
}

async fn put_orchestration(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(document): Json<OrchestrationDocument>,
) -> Result<Json<OrchestrationDocument>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    // Migrate first so v2 drafts are accepted and limits see version 3.
    let document = normalize_document(document);
    check_document_limits(&document, locale)?;
    let raw = serde_json::to_string(&document)
        .map_err(|error| ApiError::internal_logged(locale, error))?;
    chaos_store::set_meta(&state.pool, chaos_store::META_ORCHESTRATION_DRAFT, &raw).await?;
    Ok(Json(document))
}

async fn validate_document(
    _user: AuthUser,
    RequestLocale(locale): RequestLocale,
    Json(document): Json<OrchestrationDocument>,
) -> Result<Json<ValidationReport>, ApiError> {
    let document = normalize_document(document);
    check_document_limits(&document, locale)?;
    Ok(Json(document.validate()))
}

async fn publish_document(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(document): Json<OrchestrationDocument>,
) -> Result<Json<PublishResponse>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    let document = normalize_document(document);
    check_document_limits(&document, locale)?;
    let catalog = SourceCatalog::load(&state).await?;
    let (document, plan) = prepare_publish_plan(document, &catalog)
        .map_err(|error| map_prepare_error(error, locale))?;
    let failed_draft = serde_json::to_string(&document)
        .map_err(|error| ApiError::internal_logged(locale, error))?;
    let snapshot = chaos_store::snapshot_orchestration_publication(&state.pool).await?;
    let pending = serde_json::to_string(&snapshot)
        .map_err(|error| ApiError::internal_logged(locale, error))?;
    chaos_store::set_meta(
        &state.pool,
        chaos_store::META_ORCHESTRATION_PENDING,
        &pending,
    )
    .await?;

    if let Err(error) = chaos_store::publish_orchestration_v2(&state.pool, &plan).await {
        if let Err(clear_error) =
            chaos_store::delete_meta(&state.pool, chaos_store::META_ORCHESTRATION_PENDING).await
        {
            tracing::error!(error = %clear_error, "failed to clear aborted orchestration publication");
        }
        return Err(ApiError::from(error));
    }
    let applied = match crate::routes::runtime::apply_current_config_locked(&state, locale).await {
        Ok(applied) => applied,
        Err(error) => {
            let restored = chaos_store::restore_orchestration_publication(
                &state.pool,
                &snapshot,
                &failed_draft,
            )
            .await;
            if let Err(restore_error) = &restored {
                tracing::error!(error = %restore_error, "failed to restore orchestration publication");
            }
            return Err(if restored.is_ok() {
                error.with_draft_saved()
            } else {
                error
            });
        }
    };
    chaos_store::delete_meta(&state.pool, chaos_store::META_ORCHESTRATION_PENDING).await?;
    Ok(Json(PublishResponse { document, applied }))
}

pub(crate) async fn recover_pending_publication(state: &AppState) -> anyhow::Result<()> {
    let Some(raw) =
        chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_PENDING).await?
    else {
        return Ok(());
    };
    let snapshot: chaos_store::OrchestrationPublicationSnapshot = serde_json::from_str(&raw)?;
    let current_flow =
        chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_FLOW).await?;

    // The marker can be durable before the publication transaction starts.
    // If the active flow is still the snapshot, no candidate was committed.
    if current_flow == snapshot.flow {
        chaos_store::delete_meta(&state.pool, chaos_store::META_ORCHESTRATION_PENDING).await?;
        return Ok(());
    }

    tracing::warn!("recovering interrupted orchestration publication");
    match crate::routes::runtime::apply_current_config(state, chaos_i18n::Locale::En).await {
        Ok(_) => {
            chaos_store::delete_meta(&state.pool, chaos_store::META_ORCHESTRATION_PENDING).await?;
            tracing::info!("interrupted orchestration publication applied successfully");
        }
        Err(error) => {
            tracing::error!(code = error.code, message = %error.message, "pending publication apply failed; restoring previous snapshot");
            let failed_draft =
                chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_DRAFT)
                    .await?
                    .or(current_flow)
                    .or_else(|| snapshot.draft.clone())
                    .unwrap_or_else(|| {
                        serde_json::to_string(&OrchestrationDocument::default()).unwrap()
                    });
            chaos_store::restore_orchestration_publication(&state.pool, &snapshot, &failed_draft)
                .await?;
            if let Err(restore_apply_error) =
                crate::routes::runtime::apply_current_config(state, chaos_i18n::Locale::En).await
            {
                tracing::error!(
                    code = restore_apply_error.code,
                    message = %restore_apply_error.message,
                    "failed to re-apply the restored orchestration snapshot"
                );
            }
        }
    }
    Ok(())
}

fn prepare_publish_plan(
    mut document: OrchestrationDocument,
    catalog: &SourceCatalog,
) -> Result<(OrchestrationDocument, PublishedOrchestrationPlan), PrepareError> {
    let compiled = document.compile().map_err(PrepareError::Validation)?;
    let expanded = expand_document_groups(&document, catalog).map_err(PrepareError::Resource)?;
    assign_runtime_group_ids(&mut document);

    let groups: Vec<PublishedGroup> = document
        .nodes
        .iter()
        .filter(|node| node.kind == FlowNodeKind::NodeGroup)
        .map(|node| PublishedGroup {
            node_id: node.id.clone(),
            id: node
                .data
                .runtime_group_id
                .clone()
                .expect("runtime group id assigned"),
            name: node.data.name.trim().to_string(),
            policy: node.data.policy.trim().to_string(),
            members: expanded
                .get(&node.id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|(node_id, weight)| (node_id, i64::from(weight)))
                .collect(),
        })
        .collect();

    let document_json = serde_json::to_string(&document).map_err(PrepareError::Serialization)?;
    let routing = compiled
        .conditions
        .into_iter()
        .map(|route| PublishedRoutingRule {
            expression: route.condition,
            outbound: route.outbound,
        })
        .collect();
    Ok((
        document,
        PublishedOrchestrationPlan {
            document: document_json,
            groups,
            routing,
        },
    ))
}

fn expand_document_groups(
    document: &OrchestrationDocument,
    catalog: &SourceCatalog,
) -> Result<HashMap<String, Vec<(String, u32)>>, PublishIssue> {
    let flow_groups: HashMap<String, &FlowNode> = document
        .nodes
        .iter()
        .filter(|node| node.kind == FlowNodeKind::NodeGroup)
        .map(|node| (node.id.clone(), node))
        .collect();
    let mut references: HashMap<String, String> = flow_groups
        .keys()
        .map(|id| (id.clone(), id.clone()))
        .collect();
    for group in flow_groups.values() {
        if let Some(runtime_id) = group.data.runtime_group_id.as_deref() {
            references.insert(runtime_id.to_string(), group.id.clone());
        }
    }

    let mut cache = HashMap::new();
    let mut visiting = HashSet::new();
    for group_id in flow_groups.keys() {
        resolve_flow_group(
            group_id,
            &flow_groups,
            &references,
            catalog,
            &mut cache,
            &mut visiting,
        )?;
    }

    Ok(cache)
}


fn resolve_flow_group(
    group_id: &str,
    flow_groups: &HashMap<String, &FlowNode>,
    references: &HashMap<String, String>,
    catalog: &SourceCatalog,
    cache: &mut HashMap<String, Vec<(String, u32)>>,
    visiting: &mut HashSet<String>,
) -> Result<Vec<(String, u32)>, PublishIssue> {
    if let Some(cached) = cache.get(group_id) {
        return Ok(cached.clone());
    }
    if !visiting.insert(group_id.to_string()) {
        return Err(PublishIssue {
            code: "group_source_cycle",
            node_id: group_id.to_string(),
        });
    }
    let Some(group) = flow_groups.get(group_id) else {
        visiting.remove(group_id);
        return Err(PublishIssue {
            code: "source_missing",
            node_id: group_id.to_string(),
        });
    };

    let result = (|| {
        let members = expand_sources_recursive(
            &group.data.sources,
            group,
            flow_groups,
            references,
            catalog,
            cache,
            visiting,
        )?;
        if members.is_empty() {
            return Err(resource_issue("group_source_empty", group));
        }
        Ok(members)
    })();
    visiting.remove(group_id);
    if let Ok(members) = &result {
        cache.insert(group_id.to_string(), members.clone());
    }
    result
}

fn expand_sources_recursive(
    sources: &[GroupSource],
    owner: &FlowNode,
    flow_groups: &HashMap<String, &FlowNode>,
    references: &HashMap<String, String>,
    catalog: &SourceCatalog,
    cache: &mut HashMap<String, Vec<(String, u32)>>,
    visiting: &mut HashSet<String>,
) -> Result<Vec<(String, u32)>, PublishIssue> {
    let mut members = Vec::<(String, u32)>::new();
    let mut member_indexes = HashMap::<String, usize>::new();
    for source in sources {
        let source_members = match source {
            GroupSource::Node { id, weight } => {
                if !catalog.node_ids.contains(id) {
                    return Err(resource_issue("source_missing", owner));
                }
                vec![(id.clone(), *weight)]
            }
            GroupSource::Subscription { id, weight } => {
                if !catalog.subscription_ids.contains(id) {
                    return Err(resource_issue("source_missing", owner));
                }
                catalog
                    .nodes_by_subscription
                    .get(id)
                    .into_iter()
                    .flatten()
                    .map(|node_id| (node_id.clone(), *weight))
                    .collect()
            }
            GroupSource::Group { id, weight } => {
                let nested = if let Some(flow_group_id) = references.get(id) {
                    resolve_flow_group(
                        flow_group_id,
                        flow_groups,
                        references,
                        catalog,
                        cache,
                        visiting,
                    )?
                } else {
                    if !catalog.group_ids.contains(id) {
                        return Err(resource_issue("source_missing", owner));
                    }
                    catalog
                        .members_by_group
                        .get(id)
                        .cloned()
                        .unwrap_or_default()
                };
                nested
                    .into_iter()
                    .map(|(node_id, nested_weight)| {
                        (node_id, nested_weight.saturating_mul(*weight).clamp(1, 99))
                    })
                    .collect()
            }
        };
        for (node_id, weight) in source_members {
            if !catalog.node_ids.contains(&node_id) {
                return Err(resource_issue("source_missing", owner));
            }
            merge_member(&mut members, &mut member_indexes, node_id, weight);
        }
    }
    Ok(members)
}

fn merge_member(
    members: &mut Vec<(String, u32)>,
    indexes: &mut HashMap<String, usize>,
    node_id: String,
    weight: u32,
) {
    let weight = weight.clamp(1, 99);
    if let Some(index) = indexes.get(&node_id).copied() {
        members[index].1 = members[index].1.saturating_add(weight).min(99);
        return;
    }
    indexes.insert(node_id.clone(), members.len());
    members.push((node_id, weight));
}

fn resource_issue(code: &'static str, group: &FlowNode) -> PublishIssue {
    PublishIssue {
        code,
        node_id: group.id.clone(),
    }
}

fn assign_runtime_group_ids(document: &mut OrchestrationDocument) {
    let mut used = HashSet::new();
    for node in &mut document.nodes {
        if node.kind != FlowNodeKind::NodeGroup {
            continue;
        }
        let candidate = node
            .data
            .runtime_group_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        let runtime_id = match candidate {
            Some(id) if used.insert(id.clone()) => id,
            _ => loop {
                let id = Uuid::new_v4().to_string();
                if used.insert(id.clone()) {
                    break id;
                }
            },
        };
        node.data.runtime_group_id = Some(runtime_id);
    }
}

fn normalize_document(mut document: OrchestrationDocument) -> OrchestrationDocument {
    document = chaos_core::orchestration::migrate_orchestration_document(document);
    let runtime_refs: HashMap<String, String> = document
        .nodes
        .iter()
        .filter(|node| node.kind == FlowNodeKind::NodeGroup)
        .filter_map(|node| {
            node.data
                .runtime_group_id
                .as_ref()
                .map(|runtime_id| (runtime_id.clone(), node.id.clone()))
        })
        .collect();
    for node in &mut document.nodes {
        if node.kind == FlowNodeKind::NodeGroup {
            for source in &mut node.data.sources {
                rewrite_group_ref(source, &runtime_refs);
            }
        }
    }
    document
}

fn rewrite_group_ref(source: &mut GroupSource, runtime_refs: &HashMap<String, String>) {
    if let GroupSource::Group { id, .. } = source {
        if let Some(flow_node_id) = runtime_refs.get(id) {
            *id = flow_node_id.clone();
        }
    }
}

fn map_prepare_error(error: PrepareError, locale: chaos_i18n::Locale) -> ApiError {
    match error {
        PrepareError::Validation(report) => {
            tracing::warn!(issues = ?report.issues, "orchestration publication rejected");
            ApiError::bad_request("orchestration_invalid", locale)
        }
        PrepareError::Resource(issue) => {
            tracing::warn!(
                code = issue.code,
                node_id = issue.node_id,
                "orchestration resources rejected"
            );
            ApiError::bad_request("orchestration_invalid", locale)
        }
        PrepareError::Serialization(error) => ApiError::internal_logged(locale, error),
    }
}

fn check_document_limits(
    document: &OrchestrationDocument,
    locale: chaos_i18n::Locale,
) -> Result<(), ApiError> {
    let source_count: usize = document
        .nodes
        .iter()
        .map(|node| node.data.sources.len() + node.data.hops.len())
        .sum();
    if document.version != ORCHESTRATION_VERSION
        || document.nodes.len() > MAX_FLOW_NODES
        || document.edges.len() > MAX_FLOW_EDGES
        || source_count > MAX_FLOW_SOURCES
    {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let oversized_node = document.nodes.iter().any(|node| {
        node.id.len() > 128
            || node.data.name.len() > 256
            || node.data.policy.len() > 64
            || node
                .data
                .matcher
                .as_ref()
                .is_some_and(|matcher| matcher.pattern.len() > 4_096)
            || node
                .data
                .runtime_group_id
                .as_ref()
                .is_some_and(|id| id.len() > 128)
            || node
                .data
                .sources
                .iter()
                .any(|source| source.id().len() > 128)
            || node
                .data
                .hops
                .iter()
                .any(|hop| hop.id().len() > 128)
    });
    let oversized_edge = document
        .edges
        .iter()
        .any(|edge| edge.id.len() > 128 || edge.source.len() > 128 || edge.target.len() > 128);
    if oversized_node || oversized_edge {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "body_too_large",
            chaos_i18n::error_message(locale, "body_too_large"),
        ));
    }
    Ok(())
}

pub(crate) async fn orchestration_references_source(
    state: &AppState,
    kind: &str,
    id: &str,
) -> Result<bool, ApiError> {
    documents_reference_source(
        state,
        &[
            chaos_store::META_ORCHESTRATION_DRAFT,
            chaos_store::META_ORCHESTRATION_FLOW,
        ],
        kind,
        id,
    )
    .await
}

pub(crate) async fn orchestration_references_any_source(
    state: &AppState,
    kind: &str,
    ids: &HashSet<String>,
) -> Result<bool, ApiError> {
    for id in ids {
        if orchestration_references_source(state, kind, id).await? {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn documents_reference_source(
    state: &AppState,
    keys: &[&str],
    kind: &str,
    id: &str,
) -> Result<bool, ApiError> {
    for key in keys {
        let Some(raw) = chaos_store::get_meta(&state.pool, key).await? else {
            continue;
        };
        let document: OrchestrationDocument = serde_json::from_str(&raw)
            .map_err(|error| ApiError::internal_logged(chaos_i18n::Locale::En, error))?;
        if document_references_source(&document, kind, id) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn source_kind_label(source: &GroupSource) -> &'static str {
    match source {
        GroupSource::Node { .. } => "node",
        GroupSource::Subscription { .. } => "subscription",
        GroupSource::Group { .. } => "group",
    }
}

fn document_references_source(document: &OrchestrationDocument, kind: &str, id: &str) -> bool {
    document.nodes.iter().any(|node| match node.kind {
        FlowNodeKind::NodeGroup => node
            .data
            .sources
            .iter()
            .any(|source| source_kind_label(source) == kind && source.id() == id),
        // Chain is V3-only and stripped by migration; ignore if present in raw legacy docs.
        FlowNodeKind::Chain => false,
        _ => false,
    })
}

pub(crate) async fn mark_republish_if_published_source_changed(
    state: &AppState,
    kind: &str,
    id: &str,
) -> Result<bool, ApiError> {
    let referenced =
        documents_reference_source(state, &[chaos_store::META_ORCHESTRATION_FLOW], kind, id)
            .await?;
    if referenced {
        chaos_store::set_meta(
            &state.pool,
            chaos_store::META_ORCHESTRATION_NEEDS_REPUBLISH,
            "true",
        )
        .await?;
    }
    Ok(referenced)
}

pub(crate) async fn mark_republish_if_published_node_added(
    state: &AppState,
    node_id: &str,
    subscription_id: Option<&str>,
    tag: Option<&str>,
) -> Result<bool, ApiError> {
    let Some(raw) =
        chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_FLOW).await?
    else {
        return Ok(false);
    };
    let document: OrchestrationDocument = serde_json::from_str(&raw)
        .map_err(|error| ApiError::internal_logged(chaos_i18n::Locale::En, error))?;
    let groups = chaos_store::list_groups(&state.pool).await?;
    let group_filters: HashMap<String, Option<String>> = groups
        .into_iter()
        .map(|group| (group.id, group.filter_tag))
        .collect();
    let source_matches = |source: &GroupSource| match source {
        GroupSource::Node { id, .. } => id == node_id,
        GroupSource::Subscription { id, .. } => subscription_id == Some(id.as_str()),
        GroupSource::Group { id, .. } => group_filters
            .get(id)
            .and_then(|filter| filter.as_deref())
            .zip(tag)
            .is_some_and(|(filter, node_tag)| filter == node_tag),
    };
    let affected = document.nodes.iter().any(|node| match node.kind {
        FlowNodeKind::NodeGroup => node.data.sources.iter().any(source_matches),
        FlowNodeKind::Chain => false,
        _ => false,
    });
    if affected {
        chaos_store::set_meta(
            &state.pool,
            chaos_store::META_ORCHESTRATION_NEEDS_REPUBLISH,
            "true",
        )
        .await?;
    }
    Ok(affected)
}

pub(crate) async fn active_plan_references_any_node(
    state: &AppState,
    node_ids: &HashSet<String>,
) -> Result<bool, ApiError> {
    let Some(raw) =
        chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_PLAN).await?
    else {
        return Ok(false);
    };
    let plan: PublishedOrchestrationPlan = serde_json::from_str(&raw)
        .map_err(|error| ApiError::internal_logged(chaos_i18n::Locale::En, error))?;
    Ok(plan.groups.iter().any(|group| {
        group
            .members
            .iter()
            .any(|(node_id, _)| node_ids.contains(node_id))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaos_core::orchestration::{
        BuiltinKind, FlowEdge, FlowNodeData, FlowPosition, FlowViewport, RuleMatcher,
        RuleMatcherKind,
    };
    use chaos_i18n::Locale;

    fn group_node(id: &str, name: &str, sources: Vec<GroupSource>) -> FlowNode {
        FlowNode {
            id: id.into(),
            kind: FlowNodeKind::NodeGroup,
            position: FlowPosition { x: 400.0, y: 0.0 },
            data: FlowNodeData {
                name: name.into(),
                sources,
                ..FlowNodeData::default()
            },
        }
    }

    fn start_node() -> FlowNode {
        FlowNode {
            id: "start".into(),
            kind: FlowNodeKind::Start,
            position: FlowPosition { x: -200.0, y: 0.0 },
            data: FlowNodeData::default(),
        }
    }

    fn end_node() -> FlowNode {
        FlowNode {
            id: "end".into(),
            kind: FlowNodeKind::End,
            position: FlowPosition { x: 900.0, y: 0.0 },
            data: FlowNodeData::default(),
        }
    }

    fn rule_node(target_pattern: &str) -> FlowNode {
        FlowNode {
            id: "rule".into(),
            kind: FlowNodeKind::Rule,
            position: FlowPosition { x: 0.0, y: 0.0 },
            data: FlowNodeData {
                matcher: Some(RuleMatcher {
                    kind: RuleMatcherKind::DomainSuffix,
                    pattern: target_pattern.into(),
                    invert: false,
                }),
                priority: Some(1),
                ..FlowNodeData::default()
            },
        }
    }

    fn direct_node() -> FlowNode {
        FlowNode {
            id: "direct".into(),
            kind: FlowNodeKind::Builtin,
            position: FlowPosition { x: 700.0, y: 0.0 },
            data: FlowNodeData {
                builtin: Some(BuiltinKind::Direct),
                ..FlowNodeData::default()
            },
        }
    }

    fn chain_node(id: &str, name: &str, hops: Vec<GroupSource>) -> FlowNode {
        FlowNode {
            id: id.into(),
            kind: FlowNodeKind::Chain,
            position: FlowPosition { x: 500.0, y: 100.0 },
            data: FlowNodeData {
                name: name.into(),
                hops,
                ..FlowNodeData::default()
            },
        }
    }

    /// v3 topology anchors + start→rule + end→direct; rule→target supplied by caller edges.
    fn publishable_doc(
        mut extra_nodes: Vec<FlowNode>,
        mut rule_target_edges: Vec<FlowEdge>,
    ) -> OrchestrationDocument {
        let mut nodes = vec![start_node(), rule_node("example.com"), direct_node(), end_node()];
        nodes.append(&mut extra_nodes);
        let mut edges = vec![
            FlowEdge::new("start-rule", "start", "rule"),
            FlowEdge::new("end-direct", "end", "direct"),
        ];
        edges.append(&mut rule_target_edges);
        OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes,
            edges,
            viewport: FlowViewport::default(),
        }
    }

    #[test]
    fn accepts_incomplete_drafts() {
        assert!(check_document_limits(&OrchestrationDocument::default(), Locale::En).is_ok());
    }

    #[test]
    fn rejects_oversized_documents() {
        let mut document = OrchestrationDocument::default();
        let seed = document.nodes[0].clone();
        document.nodes = (0..=MAX_FLOW_NODES)
            .map(|index| FlowNode {
                id: format!("node-{index}"),
                ..seed.clone()
            })
            .collect();
        assert!(check_document_limits(&document, Locale::En).is_err());
    }


    #[test]
    fn normalizes_v3_chain_documents_before_publish() {
        let catalog = SourceCatalog {
            node_ids: ["n1"].into_iter().map(str::to_string).collect(),
            ..SourceCatalog::default()
        };
        let document = OrchestrationDocument {
            version: 3,
            nodes: vec![
                start_node(),
                rule_node("example.com"),
                group_node(
                    "group",
                    "Proxy",
                    vec![GroupSource::Node {
                        id: "n1".into(),
                        weight: 1,
                    }],
                ),
                chain_node(
                    "chain-1",
                    "Via",
                    vec![GroupSource::Group {
                        id: "group".into(),
                        weight: 1,
                    }],
                ),
                direct_node(),
                end_node(),
            ],
            edges: vec![
                FlowEdge::new("start-rule", "start", "rule"),
                FlowEdge::new("rule-chain", "rule", "chain-1"),
                FlowEdge::new("end-direct", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        let normalized = normalize_document(document);
        assert_eq!(normalized.version, ORCHESTRATION_VERSION);
        assert!(!normalized.nodes.iter().any(|n| n.kind == FlowNodeKind::Chain));
        assert!(normalized
            .edges
            .iter()
            .any(|e| e.source == "rule" && e.target == "group"));
        let (document, plan) = prepare_publish_plan(normalized, &catalog).unwrap();
        assert_eq!(plan.routing[0].outbound, "Proxy");
        assert!(!document.nodes.iter().any(|n| n.kind == FlowNodeKind::Chain));
    }

    #[test]
    fn prepares_compiled_routing_and_flattens_all_source_kinds() {
        let catalog = SourceCatalog {
            node_ids: ["n1", "n2", "n3"].into_iter().map(str::to_string).collect(),
            subscription_ids: ["sub"].into_iter().map(str::to_string).collect(),
            nodes_by_subscription: HashMap::from([("sub".into(), vec!["n2".into(), "n3".into()])]),
            group_ids: ["legacy"].into_iter().map(str::to_string).collect(),
            members_by_group: HashMap::from([("legacy".into(), vec![("n3".into(), 4)])]),
        };
        let base = group_node(
            "base",
            "Base",
            vec![
                GroupSource::Node {
                    id: "n1".into(),
                    weight: 2,
                },
                GroupSource::Subscription {
                    id: "sub".into(),
                    weight: 3,
                },
            ],
        );
        let combined = group_node(
            "combined",
            "Combined Group",
            vec![
                GroupSource::Group {
                    id: "base".into(),
                    weight: 2,
                },
                GroupSource::Group {
                    id: "legacy".into(),
                    weight: 2,
                },
            ],
        );
        let document = publishable_doc(
            vec![base, combined],
            vec![FlowEdge::new("rule-combined", "rule", "combined")],
        );

        let (document, plan) = prepare_publish_plan(document, &catalog).unwrap();
        assert_eq!(plan.routing[0].expression, "domain(suffix: example.com)");
        assert_eq!(plan.routing[0].outbound, "Combined_Group");
        let combined_group = plan
            .groups
            .iter()
            .find(|g| g.node_id == "combined")
            .expect("combined group");
        assert_eq!(
            combined_group.members,
            vec![("n1".into(), 4), ("n2".into(), 6), ("n3".into(), 14)]
        );
        assert!(document
            .nodes
            .iter()
            .filter(|node| node.kind == FlowNodeKind::NodeGroup)
            .all(|node| node
                .data
                .runtime_group_id
                .as_deref()
                .is_some_and(|id| !id.is_empty())));
        let persisted: serde_json::Value = serde_json::from_str(&plan.document).unwrap();
        assert_eq!(persisted["version"], ORCHESTRATION_VERSION);
        assert!(persisted["edges"]
            .as_array()
            .unwrap()
            .iter()
            .all(|edge| edge.get("data").is_none()));
    }

    #[test]
    fn rejects_missing_sources_during_publication() {
        let document = publishable_doc(
            vec![group_node(
                "group",
                "Proxy",
                vec![GroupSource::Node {
                    id: "missing".into(),
                    weight: 1,
                }],
            )],
            vec![FlowEdge::new("rule-group", "rule", "group")],
        );
        let error = prepare_publish_plan(document, &SourceCatalog::default()).unwrap_err();
        assert!(matches!(
            error,
            PrepareError::Resource(PublishIssue {
                code: "source_missing",
                ..
            })
        ));
    }

    #[test]
    fn normalizes_legacy_documents_to_current_version() {
        let document = OrchestrationDocument {
            version: 2,
            nodes: vec![
                rule_node("example.com"),
                group_node(
                    "group",
                    "Proxy",
                    vec![GroupSource::Node {
                        id: "n1".into(),
                        weight: 1,
                    }],
                ),
                direct_node(),
            ],
            edges: vec![FlowEdge::new("rule-group", "rule", "group")],
            viewport: FlowViewport::default(),
        };
        let normalized = normalize_document(document);
        assert_eq!(normalized.version, ORCHESTRATION_VERSION);
        assert!(normalized
            .nodes
            .iter()
            .any(|node| node.kind == FlowNodeKind::Start));
        assert!(normalized
            .nodes
            .iter()
            .any(|node| node.kind == FlowNodeKind::End));
        assert!(normalized
            .edges
            .iter()
            .any(|edge| edge.source == "start" && edge.target == "rule"));
        assert!(check_document_limits(&normalized, Locale::En).is_ok());
    }

    #[tokio::test]
    async fn detects_draft_node_references_and_marks_changed_published_sources() {
        let pool = chaos_store::connect("sqlite::memory:").await.unwrap();
        chaos_store::migrate(&pool).await.unwrap();
        let state = AppState::new(pool, "test-secret".into());

        let draft = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                start_node(),
                end_node(),
                group_node(
                    "draft-group",
                    "Draft",
                    vec![GroupSource::Node {
                        id: "draft-node".into(),
                        weight: 1,
                    }],
                ),
                direct_node(),
            ],
            edges: vec![FlowEdge::new("end-direct", "end", "direct")],
            viewport: FlowViewport::default(),
        };
        chaos_store::set_meta(
            &state.pool,
            chaos_store::META_ORCHESTRATION_DRAFT,
            &serde_json::to_string(&draft).unwrap(),
        )
        .await
        .unwrap();
        assert!(orchestration_references_any_source(
            &state,
            "node",
            &HashSet::from(["draft-node".to_string()]),
        )
        .await
        .unwrap());

        let published = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                start_node(),
                end_node(),
                group_node(
                    "published-group",
                    "Published",
                    vec![GroupSource::Subscription {
                        id: "subscription-1".into(),
                        weight: 1,
                    }],
                ),
                direct_node(),
            ],
            edges: vec![FlowEdge::new("end-direct", "end", "direct")],
            viewport: FlowViewport::default(),
        };
        chaos_store::set_meta(
            &state.pool,
            chaos_store::META_ORCHESTRATION_FLOW,
            &serde_json::to_string(&published).unwrap(),
        )
        .await
        .unwrap();
        assert!(mark_republish_if_published_source_changed(
            &state,
            "subscription",
            "subscription-1",
        )
        .await
        .unwrap());
        assert_eq!(
            chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_NEEDS_REPUBLISH,)
                .await
                .unwrap()
                .as_deref(),
            Some("true")
        );

        chaos_store::delete_meta(&state.pool, chaos_store::META_ORCHESTRATION_NEEDS_REPUBLISH)
            .await
            .unwrap();
        let catalog_group =
            chaos_store::insert_group(&state.pool, "Tagged", "fixed", Some("edge"), 0)
                .await
                .unwrap();
        let tagged_source = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                start_node(),
                end_node(),
                group_node(
                    "tagged-source",
                    "Tagged Source",
                    vec![GroupSource::Group {
                        id: catalog_group.id,
                        weight: 1,
                    }],
                ),
                direct_node(),
            ],
            edges: vec![FlowEdge::new("end-direct", "end", "direct")],
            viewport: FlowViewport::default(),
        };
        chaos_store::set_meta(
            &state.pool,
            chaos_store::META_ORCHESTRATION_FLOW,
            &serde_json::to_string(&tagged_source).unwrap(),
        )
        .await
        .unwrap();
        assert!(
            mark_republish_if_published_node_added(&state, "new-node", None, Some("edge"),)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn startup_clears_pending_marker_when_publication_never_committed() {
        let pool = chaos_store::connect("sqlite::memory:").await.unwrap();
        chaos_store::migrate(&pool).await.unwrap();
        let state = AppState::new(pool, "test-secret".into());
        let snapshot = chaos_store::snapshot_orchestration_publication(&state.pool)
            .await
            .unwrap();
        chaos_store::set_meta(
            &state.pool,
            chaos_store::META_ORCHESTRATION_PENDING,
            &serde_json::to_string(&snapshot).unwrap(),
        )
        .await
        .unwrap();

        recover_pending_publication(&state).await.unwrap();
        assert!(
            chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_PENDING,)
                .await
                .unwrap()
                .is_none()
        );
    }
}
