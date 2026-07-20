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

use crate::auth::AuthUser;
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

#[derive(Debug, Serialize)]
pub struct SubscriptionDto {
    pub id: String,
    pub tag: Option<String>,
    pub url: String,
    pub updated_at: String,
    pub status: String,
    pub node_count: usize,
    pub needs_republish: bool,
}

impl SubscriptionDto {
    fn from_sub(s: Subscription, node_count: usize, needs_republish: bool) -> Self {
        Self {
            id: s.id,
            tag: s.tag,
            url: s.url,
            updated_at: s.updated_at,
            status: s.status,
            node_count,
            needs_republish,
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
            "/subscriptions/{id}",
            axum::routing::delete(delete_subscription_handler),
        )
}

async fn list_subscriptions_handler(
    _user: AuthUser,
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
            SubscriptionDto::from_sub(s, count, needs_republish)
        })
        .collect();

    Ok(Json(ListSubscriptionsResponse { subscriptions }))
}

async fn import_subscription(
    _user: AuthUser,
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
    _user: AuthUser,
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
    _user: AuthUser,
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

async fn fetch_and_replace_nodes(
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
    if orchestration_references_any_source(state, "node", &removed_ids).await?
        || active_plan_references_any_node(state, &removed_ids).await?
    {
        return Err(ApiError::conflict("resource_in_use", locale));
    }

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
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("chaos-api/0.1")
        .build()
        .map_err(|e| {
            tracing::error!(?e, "reqwest client build failed");
            subscription_fetch_failed(locale)
        })?;

    let mut current =
        reqwest::Url::parse(url).map_err(|_| ApiError::bad_request("invalid_url", locale))?;
    for hop in 0..=5 {
        validate_subscription_url_parsed(&current, locale).await?;
        let response = client.get(current.clone()).send().await.map_err(|e| {
            tracing::warn!(error = %e, "subscription fetch failed");
            subscription_fetch_failed(locale)
        })?;
        if response.status().is_redirection() {
            if hop == 5 {
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
    validate_subscription_url_parsed(&parsed, locale).await
}

async fn validate_subscription_url_parsed(
    url: &reqwest::Url,
    locale: Locale,
) -> Result<(), ApiError> {
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
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| ApiError::bad_request("invalid_url", locale))?;
    let mut found = false;
    for address in addresses {
        found = true;
        if blocked_subscription_ip(address.ip()) {
            return Err(ApiError::bad_request("invalid_url", locale));
        }
    }
    if !found {
        return Err(ApiError::bad_request("invalid_url", locale));
    }
    Ok(())
}

fn blocked_subscription_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast()
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chaos_store::{connect, migrate};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::auth::{auth_router, issue_token};

    async fn test_app() -> (Router, AppState) {
        let pool = connect("sqlite::memory:").await.unwrap();
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
}
