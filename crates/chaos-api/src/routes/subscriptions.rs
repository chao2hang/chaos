//! Subscription list / import / refresh / delete routes (auth required).

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
}

impl SubscriptionDto {
    fn from_sub(s: Subscription, node_count: usize) -> Self {
        Self {
            id: s.id,
            tag: s.tag,
            url: s.url,
            updated_at: s.updated_at,
            status: s.status,
            node_count,
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
        .route(
            "/subscriptions/{id}/refresh",
            post(refresh_subscription),
        )
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

    let subscriptions = subs
        .into_iter()
        .map(|s| {
            let count = all_nodes
                .iter()
                .filter(|n| n.subscription_id.as_deref() == Some(s.id.as_str()))
                .count();
            SubscriptionDto::from_sub(s, count)
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
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(ApiError::bad_request("invalid_url", locale));
    }

    let tag = body
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());

    let sub = insert_subscription(&state.pool, tag, url, "pending").await?;

    match fetch_and_replace_nodes(&state, &sub.id, tag, url, locale).await {
        Ok((sub, nodes)) => Ok(Json(ImportSubscriptionResponse {
            subscription: SubscriptionDto::from_sub(sub, nodes.len()),
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
            subscription: SubscriptionDto::from_sub(sub, nodes.len()),
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

    let mut new_nodes = Vec::with_capacity(links.len());
    for link in links {
        let protocol = chaos_core::link::detect_protocol(&link);
        let address = chaos_core::link::detect_address(&link);
        let id = Uuid::new_v4().to_string();
        let name = chaos_core::link::node_name(sub_tag, protocol.as_deref(), &id);
        new_nodes.push(NewSubscriptionNode {
            id: Some(id),
            name,
            tag: sub_tag.map(|t| t.to_string()),
            link,
            protocol,
            address,
        });
    }

    let nodes =
        replace_subscription_nodes(&state.pool, subscription_id, "ok", &new_nodes).await?;

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
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("chaos-api/0.1")
        .build()
        .map_err(|e| {
            tracing::error!(?e, "reqwest client build failed");
            subscription_fetch_failed(locale)
        })?;

    let response = client.get(url).send().await.map_err(|e| {
        tracing::warn!(error = %e, "subscription fetch failed");
        subscription_fetch_failed(locale)
    })?;

    if !response.status().is_success() {
        return Err(subscription_fetch_failed(locale));
    }

    read_body_capped(response, MAX_BODY_BYTES, locale).await
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
