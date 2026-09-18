//! Subscription list / import / refresh / delete routes (auth required).

use std::collections::HashSet;
use std::net::IpAddr;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chaos_i18n::Locale;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use chaos_core::subscription::{decode_subscription_body, parse_subscription_links};
use chaos_store::{
    delete_subscription, get_subscription, insert_subscription, list_subscriptions,
    replace_subscription_nodes, update_subscription_meta, NewSubscriptionNode, Node, Subscription,
};

use crate::auth::{AdminUser, AuthUser};
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::routes::orchestration::{
    active_plan_references_any_node, mark_republish_if_published_node_added,
    mark_republish_if_published_source_changed, orchestration_references_any_source,
    orchestration_references_source,
};
use crate::state::AppState;

const FETCH_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_BODY_BYTES: usize = 5 * 1024 * 1024; // 5 MiB
const MAX_REDIRECT_HOPS: usize = 5;
const SUBSCRIPTION_USER_AGENT: &str = "chaos-api/0.1";

#[derive(Debug, Serialize)]
pub struct SubscriptionDto {
    pub id: String,
    pub tag: Option<String>,
    pub url: String,
    pub updated_at: String,
    pub status: String,
    pub node_count: usize,
    pub needs_republish: bool,
    pub refresh_interval_hours: i64,
    pub last_refreshed_at: Option<String>,
    pub next_refresh_at: Option<String>,
}

impl SubscriptionDto {
    /// `redact_url` must be set for callers that are not administrators: a
    /// subscription URL embeds its access token in the path, so a low-privileged
    /// reader must not receive a usable credential.
    fn from_sub(
        s: Subscription,
        node_count: usize,
        needs_republish: bool,
        redact_url: bool,
    ) -> Self {
        let url = if redact_url {
            chaos_core::subscription::redact_subscription_url_for_log(&s.url)
        } else {
            s.url
        };
        Self {
            id: s.id,
            tag: s.tag,
            url,
            updated_at: s.updated_at,
            status: s.status,
            node_count,
            needs_republish,
            refresh_interval_hours: s.refresh_interval_hours,
            last_refreshed_at: s.last_refreshed_at,
            next_refresh_at: s.next_refresh_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListSubscriptionsResponse {
    pub subscriptions: Vec<SubscriptionDto>,
}

#[derive(Debug, Deserialize)]
pub struct ImportSubscriptionRequest {
    pub url: String,
    pub tag: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportSubscriptionResponse {
    pub subscription: SubscriptionDto,
    pub nodes: Vec<NodeDto>,
}

#[derive(Debug, Serialize)]
pub struct NodeDto {
    pub id: String,
    pub name: String,
    pub tag: Option<String>,
    pub link: String,
    pub protocol: Option<String>,
    pub address: Option<String>,
    pub subscription_id: Option<String>,
    pub created_at: String,
    pub country_code: Option<String>,
}

impl From<Node> for NodeDto {
    fn from(n: Node) -> Self {
        Self {
            id: n.id,
            name: n.name,
            tag: n.tag,
            link: n.link,
            protocol: n.protocol,
            address: n.address,
            subscription_id: n.subscription_id,
            created_at: n.created_at,
            country_code: n.country_code,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DeleteSubscriptionResponse {
    pub deleted: bool,
}

pub fn subscriptions_router() -> Router<AppState> {
    Router::new()
        .route(
            "/subscriptions",
            get(list_subscriptions_handler).post(import_subscription),
        )
        .route("/subscriptions/{id}/refresh", post(refresh_subscription))
        .route(
            "/subscriptions/{id}/refresh-schedule",
            axum::routing::put(set_refresh_schedule),
        )
        .route(
            "/subscriptions/{id}",
            axum::routing::delete(delete_subscription_handler),
        )
}

#[derive(Debug, Deserialize)]
pub struct SetRefreshScheduleRequest {
    pub refresh_interval_hours: i64,
}

#[derive(Debug, Serialize)]
pub struct SetRefreshScheduleResponse {
    pub subscription: SubscriptionDto,
}

async fn set_refresh_schedule(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
    Json(body): Json<SetRefreshScheduleRequest>,
) -> Result<Json<SetRefreshScheduleResponse>, ApiError> {
    let hours = body.refresh_interval_hours.clamp(0, 8760);
    let sub = chaos_store::set_subscription_refresh_schedule(&state.pool, &id, hours)
        .await?
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;

    let node_count = chaos_store::list_nodes(&state.pool)
        .await?
        .iter()
        .filter(|n| n.subscription_id.as_deref() == Some(id.as_str()))
        .count();
    let needs_republish =
        chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_NEEDS_REPUBLISH)
            .await?
            .as_deref()
            == Some("true");

    Ok(Json(SetRefreshScheduleResponse {
        subscription: SubscriptionDto::from_sub(sub, node_count, needs_republish, false),
    }))
}

async fn list_subscriptions_handler(
    user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ListSubscriptionsResponse>, ApiError> {
    let subs = list_subscriptions(&state.pool).await?;
    let all_nodes = chaos_store::list_nodes(&state.pool).await?;
    let needs_republish =
        chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_NEEDS_REPUBLISH)
            .await?
            .as_deref()
            == Some("true");

    let subscriptions = subs
        .into_iter()
        .map(|s| {
            let count = all_nodes
                .iter()
                .filter(|n| n.subscription_id.as_deref() == Some(s.id.as_str()))
                .count();
            SubscriptionDto::from_sub(s, count, needs_republish, !user.is_admin())
        })
        .collect();

    Ok(Json(ListSubscriptionsResponse { subscriptions }))
}

async fn import_subscription(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<ImportSubscriptionRequest>,
) -> Result<Json<ImportSubscriptionResponse>, ApiError> {
    let url = body.url.trim();
    if url.is_empty() {
        return Err(ApiError::bad_request("empty_url", locale));
    }
    validate_subscription_url(url, locale).await?;

    let tag = body.tag.as_deref().map(str::trim).filter(|t| !t.is_empty());

    let sub = {
        let _runtime_guard = state.runtime_lock.lock().await;
        insert_subscription(&state.pool, tag, url, "pending").await?
    };

    match fetch_and_replace_nodes(&state, &sub.id, tag, url, locale).await {
        Ok((sub, nodes)) => Ok(Json(ImportSubscriptionResponse {
            subscription: SubscriptionDto::from_sub(
                sub,
                nodes.len(),
                chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_NEEDS_REPUBLISH)
                    .await?
                    .as_deref()
                    == Some("true"),
                // Admin-only endpoint: the caller may see the real URL.
                false,
            ),
            nodes: nodes.into_iter().map(NodeDto::from).collect(),
        })),
        Err(e) => {
            let _ = update_subscription_meta(&state.pool, &sub.id, "error").await;
            Err(e)
        }
    }
}

async fn refresh_subscription(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> Result<Json<ImportSubscriptionResponse>, ApiError> {
    let sub = get_subscription(&state.pool, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;

    let tag = sub.tag.as_deref();
    match fetch_and_replace_nodes(&state, &sub.id, tag, &sub.url, locale).await {
        Ok((sub, nodes)) => Ok(Json(ImportSubscriptionResponse {
            subscription: SubscriptionDto::from_sub(
                sub,
                nodes.len(),
                chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_NEEDS_REPUBLISH)
                    .await?
                    .as_deref()
                    == Some("true"),
                // Admin-only endpoint: the caller may see the real URL.
                false,
            ),
            nodes: nodes.into_iter().map(NodeDto::from).collect(),
        })),
        Err(e) => {
            let _ = update_subscription_meta(&state.pool, &sub.id, "error").await;
            Err(e)
        }
    }
}

async fn delete_subscription_handler(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> Result<Json<DeleteSubscriptionResponse>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    if orchestration_references_source(&state, "subscription", &id).await? {
        return Err(ApiError::conflict("resource_in_use", locale));
    }
    let subscription_nodes = chaos_store::list_nodes(&state.pool)
        .await?
        .into_iter()
        .filter(|node| node.subscription_id.as_deref() == Some(id.as_str()))
        .map(|node| node.id)
        .collect::<HashSet<_>>();
    if active_plan_references_any_node(&state, &subscription_nodes).await? {
        return Err(ApiError::conflict("resource_in_use", locale));
    }
    let deleted = delete_subscription(&state.pool, &id).await?;
    if !deleted {
        return Err(ApiError::not_found("not_found", locale));
    }
    Ok(Json(DeleteSubscriptionResponse { deleted: true }))
}

/// Shared by the manual refresh route and the background auto-refresh task.
/// Reuses node ids for unchanged links, guards nodes that runtime configuration
/// references, and bumps republish markers when a published plan is affected.
pub(crate) async fn fetch_and_replace_nodes(
    state: &AppState,
    subscription_id: &str,
    sub_tag: Option<&str>,
    url: &str,
    locale: Locale,
) -> Result<(Subscription, Vec<Node>), ApiError> {
    let body = fetch_subscription_body(url, locale).await?;
    let text = decode_subscription_body(&body);
    let links = parse_subscription_links(&text);

    // Reuse IDs for unchanged links so a published plan does not lose all of its
    // members on every refresh.
    let existing = chaos_store::list_nodes(&state.pool).await?;
    let mut ids_by_link: std::collections::HashMap<String, std::collections::VecDeque<String>> =
        std::collections::HashMap::new();
    for node in existing
        .into_iter()
        .filter(|node| node.subscription_id.as_deref() == Some(subscription_id))
    {
        ids_by_link.entry(node.link).or_default().push_back(node.id);
    }

    let mut new_nodes = Vec::with_capacity(links.len());
    for link in links {
        let protocol = chaos_core::link::detect_protocol(&link);
        let address = chaos_core::link::detect_address(&link);
        let link_tag = chaos_core::link::detect_tag(&link);
        let id = ids_by_link
            .get_mut(&link)
            .and_then(std::collections::VecDeque::pop_front)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let name =
            chaos_core::link::node_name(link_tag.as_deref(), sub_tag, protocol.as_deref(), &id);
        new_nodes.push(NewSubscriptionNode {
            id: Some(id),
            name,
            tag: link_tag,
            link,
            protocol,
            address,
        });
    }

    let old_node_ids: HashSet<String> = chaos_store::list_nodes(&state.pool)
        .await?
        .into_iter()
        .filter(|node| node.subscription_id.as_deref() == Some(subscription_id))
        .map(|node| node.id)
        .collect();
    let _runtime_guard = state.runtime_lock.lock().await;
    let new_node_ids: HashSet<String> = new_nodes
        .iter()
        .filter_map(|node| node.id.clone())
        .collect();
    let added_ids: HashSet<String> = new_node_ids.difference(&old_node_ids).cloned().collect();
    let removed_ids: HashSet<String> = old_node_ids.difference(&new_node_ids).cloned().collect();
    ensure_removed_nodes_unreferenced(state, &removed_ids, locale).await?;

    let mut nodes =
        replace_subscription_nodes(&state.pool, subscription_id, "ok", &new_nodes).await?;

    if !added_ids.is_empty() {
        let _ = mark_republish_if_published_source_changed(state, "subscription", subscription_id)
            .await?;
        for node in new_nodes
            .iter()
            .filter(|node| node.id.as_deref().is_some_and(|id| added_ids.contains(id)))
        {
            let _ = mark_republish_if_published_node_added(
                state,
                node.id.as_deref().unwrap_or_default(),
                Some(subscription_id),
                node.tag.as_deref(),
            )
            .await?;
        }
    }

    drop(_runtime_guard);

    // GeoIP: look up country codes for imported nodes.
    let geo_pairs: Vec<(String, String)> = nodes
        .iter()
        .filter_map(|n| n.address.as_ref().map(|a| (n.id.clone(), a.clone())))
        .collect();

    if chaos_core::geoip::enabled() && !geo_pairs.is_empty() {
        let geo = chaos_core::geoip::batch_lookup_country(&geo_pairs).await;
        for (node_id, cc) in &geo {
            let _ = chaos_store::update_node_country_code(&state.pool, node_id, cc).await;
        }
        // Patch country_code into returned nodes.
        for n in &mut nodes {
            if let Some(cc) = geo.get(&n.id) {
                n.country_code = Some(cc.clone());
            }
        }
    }

    let sub = get_subscription(&state.pool, subscription_id)
        .await?
        .ok_or_else(|| {
            ApiError::internal_logged(locale, "subscription disappeared after replace")
        })?;

    Ok((sub, nodes))
}

/// Fail closed when a refresh would delete nodes that runtime configuration
/// still depends on: nodes referenced by an orchestration document, by the
/// published plan, or by a source group. Deleting them would silently strip
/// configured members from the live config.
async fn ensure_removed_nodes_unreferenced(
    state: &AppState,
    removed_ids: &HashSet<String>,
    locale: Locale,
) -> Result<(), ApiError> {
    if removed_ids.is_empty() {
        return Ok(());
    }
    let group_referenced = chaos_store::list_all_group_members(&state.pool)
        .await?
        .iter()
        .any(|member| removed_ids.contains(&member.node_id));
    if orchestration_references_any_source(state, "node", removed_ids).await?
        || active_plan_references_any_node(state, removed_ids).await?
        || group_referenced
    {
        return Err(ApiError::conflict("resource_in_use", locale));
    }
    Ok(())
}

fn subscription_fetch_failed(locale: Locale) -> ApiError {
    ApiError::new(
        StatusCode::BAD_GATEWAY,
        "subscription_fetch_failed",
        chaos_i18n::error_message(locale, "subscription_fetch_failed"),
    )
}

/// Append a chunk while enforcing `max` without allocating past the limit.
fn append_body_chunk(buf: &mut Vec<u8>, chunk: &[u8], max: usize) -> Result<(), ApiError> {
    let next = buf.len().saturating_add(chunk.len());
    if next > max {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "body_too_large",
            chaos_i18n::error_message(Locale::En, "body_too_large"),
        ));
    }
    buf.reserve(chunk.len());
    buf.extend_from_slice(chunk);
    Ok(())
}

async fn read_body_capped(
    mut response: reqwest::Response,
    max: usize,
    locale: Locale,
) -> Result<Vec<u8>, ApiError> {
    if let Some(len) = response.content_length() {
        if len as usize > max {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "body_too_large",
                chaos_i18n::error_message(locale, "body_too_large"),
            ));
        }
    }

    let mut body = Vec::new();
    loop {
        let chunk = response.chunk().await.map_err(|e| {
            tracing::warn!(error = %e, "subscription body read failed");
            subscription_fetch_failed(locale)
        })?;
        let Some(chunk) = chunk else {
            break;
        };
        if let Err(e) = append_body_chunk(&mut body, &chunk, max) {
            // re-localize body_too_large with request locale
            if e.code == "body_too_large" {
                return Err(ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "body_too_large",
                    chaos_i18n::error_message(locale, "body_too_large"),
                ));
            }
            return Err(e);
        }
    }
    Ok(body)
}

async fn fetch_subscription_body(url: &str, locale: Locale) -> Result<Vec<u8>, ApiError> {
    let mut current =
        reqwest::Url::parse(url).map_err(|_| ApiError::bad_request("invalid_url", locale))?;
    for hop in 0..=MAX_REDIRECT_HOPS {
        let addresses = resolve_validated_addresses(&current, locale).await?;
        let host = current.host_str().unwrap_or_default().to_string();
        // Pin the connection to the addresses that were just validated. Handing
        // the host name to the client would let a second DNS answer (a rebinding
        // attacker's) send the request somewhere the validation never approved.
        // TLS still verifies the certificate against the host name, so this
        // narrows where we connect without weakening identity checks.
        let client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(SUBSCRIPTION_USER_AGENT)
            .resolve_to_addrs(&host, &addresses)
            .build()
            .map_err(|error| {
                tracing::error!(%error, "subscription fetch client build failed");
                subscription_fetch_failed(locale)
            })?;
        let response = client.get(current.clone()).send().await.map_err(|e| {
            tracing::warn!(error = %e, "subscription fetch failed");
            subscription_fetch_failed(locale)
        })?;
        if response.status().is_redirection() {
            if hop == MAX_REDIRECT_HOPS {
                return Err(subscription_fetch_failed(locale));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| subscription_fetch_failed(locale))?;
            current = current
                .join(location)
                .map_err(|_| ApiError::bad_request("invalid_url", locale))?;
            continue;
        }
        if !response.status().is_success() {
            return Err(subscription_fetch_failed(locale));
        }
        return read_body_capped(response, MAX_BODY_BYTES, locale).await;
    }
    Err(subscription_fetch_failed(locale))
}

async fn validate_subscription_url(url: &str, locale: Locale) -> Result<(), ApiError> {
    let parsed =
        reqwest::Url::parse(url).map_err(|_| ApiError::bad_request("invalid_url", locale))?;
    resolve_validated_addresses(&parsed, locale)
        .await
        .map(|_| ())
}

/// Resolve `url`'s host and reject it unless every resolved address is a public
/// internet address.
///
/// The addresses are returned so the caller can pin the connection to them:
/// validating one DNS answer and then letting the HTTP client resolve again
/// leaves a rebinding window in which the second answer can point at an
/// internal host.
async fn resolve_validated_addresses(
    url: &reqwest::Url,
    locale: Locale,
) -> Result<Vec<std::net::SocketAddr>, ApiError> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ApiError::bad_request("invalid_url", locale));
    }
    let host = url.host_str().unwrap_or_default();
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        return Err(ApiError::bad_request("invalid_url", locale));
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let mut addresses = Vec::new();
    for address in tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| ApiError::bad_request("invalid_url", locale))?
    {
        if blocked_subscription_ip(address.ip()) {
            return Err(ApiError::bad_request("invalid_url", locale));
        }
        addresses.push(address);
    }
    if addresses.is_empty() {
        return Err(ApiError::bad_request("invalid_url", locale));
    }
    Ok(addresses)
}

fn blocked_subscription_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => blocked_subscription_ipv4(ip),
        IpAddr::V6(ip) => {
            // `::ffff:a.b.c.d` reaches the same host as `a.b.c.d`, but every
            // `std` predicate below looks only at the IPv6 form — `is_loopback`
            // is true for `::1` alone — so `::ffff:127.0.0.1` would otherwise
            // sail past as a public address. Judge the mapped form as IPv4.
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return blocked_subscription_ipv4(mapped);
            }
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
    }
}

/// Ranges that must never be reached by a fetched subscription URL.
///
/// Beyond the RFC 1918 / loopback / link-local set that `std` provides, this
/// covers the blocks that are also unreachable as public addresses but sit next
/// to internal services: carrier-grade NAT (`100.64.0.0/10`), benchmarking
/// (`198.18.0.0/15`), "this network" (`0.0.0.0/8`) and IETF/reserved space.
fn blocked_subscription_ipv4(ip: std::net::Ipv4Addr) -> bool {
    let [first, second, third, _] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_broadcast()
        || first == 0
        || (first == 100 && (64..128).contains(&second))
        || (first == 198 && (18..20).contains(&second))
        || (first == 192 && second == 0 && third == 0)
        || first >= 240
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chaos_store::{connect, migrate};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::auth::{auth_router, issue_token, issue_token_role};

    async fn test_app() -> (Router, AppState) {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        // `AdminUser` reads the role from the users table rather than the token,
        // so the non-admin case needs a real row to exist.
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
        let app = Router::new()
            .nest("/api/v1/auth", auth_router())
            .nest("/api/v1", subscriptions_router())
            .with_state(state.clone());
        (app, state)
    }

    async fn json_body(res: axum::response::Response) -> serde_json::Value {
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn blocked_subscription_ips_cover_mapped_and_non_public_ranges() {
        for blocked in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "198.18.0.1",
            "0.0.0.0",
            "0.1.2.3",
            "192.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::1",
            "::",
            "fc00::1",
            "fe80::1",
            // IPv4-mapped forms reach the same host as the embedded IPv4
            // address, so they must be judged as one.
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::ffff:100.64.0.1",
        ] {
            let ip: IpAddr = blocked.parse().unwrap();
            assert!(blocked_subscription_ip(ip), "{blocked} should be blocked");
        }

        for allowed in ["1.1.1.1", "93.184.216.34", "2606:4700::1111"] {
            let ip: IpAddr = allowed.parse().unwrap();
            assert!(
                !blocked_subscription_ip(ip),
                "{allowed} should be allowed as a public address"
            );
        }
    }

    #[test]
    fn append_body_chunk_accepts_within_limit() {
        let mut buf = Vec::new();
        append_body_chunk(&mut buf, b"hello", 10).unwrap();
        append_body_chunk(&mut buf, b"!", 10).unwrap();
        assert_eq!(buf, b"hello!");
    }

    #[test]
    fn append_body_chunk_rejects_oversize_without_growing_past_cap() {
        let mut buf = vec![0u8; 8];
        let err = append_body_chunk(&mut buf, &[1u8; 4], 10).unwrap_err();
        assert_eq!(err.code, "body_too_large");
        assert_eq!(buf.len(), 8);
    }

    #[test]
    fn append_body_chunk_rejects_exact_overflow_from_empty() {
        let mut buf = Vec::new();
        let chunk = vec![0u8; MAX_BODY_BYTES + 1];
        let err = append_body_chunk(&mut buf, &chunk, MAX_BODY_BYTES).unwrap_err();
        assert_eq!(err.code, "body_too_large");
        assert!(buf.is_empty());
    }

    #[tokio::test]
    async fn non_admin_cannot_write_subscriptions_and_sees_no_url_secret() {
        let (app, state) = test_app().await;
        let url = "https://subs.example.com/secret-access-token";
        let sub = chaos_store::insert_subscription(&state.pool, Some("s1"), url, "ok")
            .await
            .unwrap();

        let admin = issue_token("u1", "admin", &state.jwt_secret).unwrap();
        let viewer = issue_token_role("u2", "viewer", "user", &state.jwt_secret).unwrap();

        // A non-admin may list subscriptions, but the URL embeds the access
        // token, so it must come back redacted.
        let list = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/subscriptions")
                    .header("authorization", format!("Bearer {viewer}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let listed = json_body(list).await["subscriptions"][0]["url"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            !listed.contains("secret-access-token"),
            "non-admin response leaked the token: {listed}"
        );
        assert_eq!(listed, "https://subs.example.com/<redacted>");

        // An admin still gets the real URL.
        let list = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/subscriptions")
                    .header("authorization", format!("Bearer {admin}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            json_body(list).await["subscriptions"][0]["url"],
            serde_json::json!(url)
        );

        // Writes are admin-only; the rejection happens during extraction, so
        // the handler never runs.
        let refresh = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/subscriptions/{}/refresh", sub.id))
                    .header("authorization", format!("Bearer {viewer}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(refresh.status(), StatusCode::FORBIDDEN);
        assert_eq!(json_body(refresh).await["error"]["code"], "admin_required");
    }

    #[tokio::test]
    async fn subscription_urls_reject_private_network_targets() {
        for url in [
            "http://127.0.0.1/sub",
            "http://169.254.169.254/latest/meta-data",
            "https://[::1]/sub",
        ] {
            let error = validate_subscription_url(url, Locale::En)
                .await
                .expect_err("private target must be rejected");
            assert_eq!(error.code, "invalid_url");
        }
    }

    #[tokio::test]
    async fn subscriptions_require_auth() {
        let (app, _) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/subscriptions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn list_empty_with_auth() {
        let (app, state) = test_app().await;
        let token = issue_token("u1", "admin", &state.jwt_secret).unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/subscriptions")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = json_body(res).await;
        assert_eq!(body["subscriptions"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn import_rejects_empty_url() {
        let (app, state) = test_app().await;
        let token = issue_token("u1", "admin", &state.jwt_secret).unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/subscriptions")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"url":""}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn delete_missing_subscription() {
        let (app, state) = test_app().await;
        let token = issue_token("u1", "admin", &state.jwt_secret).unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/subscriptions/does-not-exist")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    /// Integration: insert subscription + replace nodes without live HTTP.
    #[tokio::test]
    async fn store_backed_refresh_path_via_replace() {
        let (app, state) = test_app().await;
        let token = issue_token("u1", "admin", &state.jwt_secret).unwrap();

        let sub = insert_subscription(&state.pool, Some("t"), "https://example.invalid/sub", "ok")
            .await
            .unwrap();
        let nodes = replace_subscription_nodes(
            &state.pool,
            &sub.id,
            "ok",
            &[NewSubscriptionNode {
                id: None,
                name: "n1".into(),
                tag: Some("t".into()),
                link: "trojan://u@1.2.3.4:443".into(),
                protocol: Some("trojan".into()),
                address: Some("1.2.3.4:443".into()),
            }],
        )
        .await
        .unwrap();
        assert_eq!(nodes.len(), 1);

        let list = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/subscriptions")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let body = json_body(list).await;
        assert_eq!(body["subscriptions"][0]["node_count"], 1);

        let del = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/subscriptions/{}", sub.id))
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(del.status(), StatusCode::OK);
        assert_eq!(json_body(del).await["deleted"], true);
    }

    /// A refresh that would delete a node referenced by a source group, an
    /// orchestration document, or the published plan must fail closed instead
    /// of silently stripping configured members.
    #[tokio::test]
    async fn removed_node_referenced_by_group_blocks_refresh() {
        let (_app, state) = test_app().await;
        let sub = insert_subscription(&state.pool, Some("t"), "https://example.invalid/sub", "ok")
            .await
            .unwrap();
        let nodes = replace_subscription_nodes(
            &state.pool,
            &sub.id,
            "ok",
            &[NewSubscriptionNode {
                id: Some("node-1".into()),
                name: "n1".into(),
                tag: Some("t".into()),
                link: "trojan://u@1.2.3.4:443".into(),
                protocol: Some("trojan".into()),
                address: Some("1.2.3.4:443".into()),
            }],
        )
        .await
        .unwrap();
        let group = chaos_store::insert_group(&state.pool, "home", "min_moving_avg", None, 0)
            .await
            .unwrap();
        chaos_store::add_group_member(&state.pool, &group.id, &nodes[0].id, 1)
            .await
            .unwrap();

        let removed: HashSet<String> = [nodes[0].id.clone()].into_iter().collect();
        let err = ensure_removed_nodes_unreferenced(&state, &removed, Locale::En)
            .await
            .expect_err("group-referenced node must not be deletable");
        assert_eq!(err.code, "resource_in_use");

        // The node is untouched.
        let remaining = chaos_store::list_nodes(&state.pool).await.unwrap();
        assert!(remaining.iter().any(|n| n.id == nodes[0].id));
    }

    /// A refresh that would delete a node referenced by the published plan
    /// must fail closed.
    #[tokio::test]
    async fn removed_node_referenced_by_plan_blocks_refresh() {
        let (_app, state) = test_app().await;
        let sub = insert_subscription(&state.pool, Some("t"), "https://example.invalid/sub", "ok")
            .await
            .unwrap();
        let nodes = replace_subscription_nodes(
            &state.pool,
            &sub.id,
            "ok",
            &[NewSubscriptionNode {
                id: Some("node-1".into()),
                name: "n1".into(),
                tag: Some("t".into()),
                link: "trojan://u@1.2.3.4:443".into(),
                protocol: Some("trojan".into()),
                address: Some("1.2.3.4:443".into()),
            }],
        )
        .await
        .unwrap();
        chaos_store::publish_orchestration_v2(
            &state.pool,
            &chaos_store::PublishedOrchestrationPlan {
                document:
                    r#"{"version":4,"nodes":[],"edges":[],"viewport":{"x":0.0,"y":0.0,"zoom":1.0}}"#
                        .to_string(),
                groups: vec![chaos_store::PublishedGroup {
                    node_id: "g1".into(),
                    id: "runtime-g1".into(),
                    name: "home".into(),
                    policy: "min_moving_avg".into(),
                    members: vec![(nodes[0].id.clone(), 1)],
                }],
                routing: vec![],
            },
        )
        .await
        .unwrap();

        let removed: HashSet<String> = [nodes[0].id.clone()].into_iter().collect();
        let err = ensure_removed_nodes_unreferenced(&state, &removed, Locale::En)
            .await
            .expect_err("plan-referenced node must not be deletable");
        assert_eq!(err.code, "resource_in_use");
    }

    /// Unreferenced removed nodes are free to be replaced.
    #[tokio::test]
    async fn removed_node_unreferenced_allows_refresh() {
        let (_app, state) = test_app().await;
        let sub = insert_subscription(&state.pool, Some("t"), "https://example.invalid/sub", "ok")
            .await
            .unwrap();
        let nodes = replace_subscription_nodes(
            &state.pool,
            &sub.id,
            "ok",
            &[NewSubscriptionNode {
                id: Some("node-1".into()),
                name: "n1".into(),
                tag: Some("t".into()),
                link: "trojan://u@1.2.3.4:443".into(),
                protocol: Some("trojan".into()),
                address: Some("1.2.3.4:443".into()),
            }],
        )
        .await
        .unwrap();

        let removed: HashSet<String> = [nodes[0].id.clone()].into_iter().collect();
        ensure_removed_nodes_unreferenced(&state, &removed, Locale::En)
            .await
            .expect("unreferenced node is deletable");
        ensure_removed_nodes_unreferenced(&state, &HashSet::new(), Locale::En)
            .await
            .expect("empty removal set is always allowed");
    }
}
