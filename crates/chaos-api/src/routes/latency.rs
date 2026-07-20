//! Latency test + list routes (auth required).

use std::time::Duration;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use chaos_core::latency::{probe_batch, resolve_probe_target};
use chaos_store::{
    list_latency_results, list_nodes, now_rfc3339, upsert_latency_result, LatencyResult, Node,
};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Serialize)]
pub struct LatencyDto {
    pub id: String,
    pub latency_ms: Option<u32>,
    pub alive: bool,
    pub tested_at: String,
    pub message: Option<String>,
}

impl From<LatencyResult> for LatencyDto {
    fn from(r: LatencyResult) -> Self {
        Self {
            id: r.node_id,
            latency_ms: r.latency_ms.and_then(|v| u32::try_from(v).ok()),
            alive: r.alive != 0,
            tested_at: r.tested_at,
            message: r.message,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListLatencyResponse {
    pub results: Vec<LatencyDto>,
}

#[derive(Debug, Deserialize)]
pub struct TestLatencyRequest {
    /// When null or omitted, test all nodes. Empty array tests none.
    #[serde(default)]
    pub ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct TestLatencyResponse {
    /// `"proxy"` when chaos-prober was used, `"tcp"` for TCP connect fallback.
    pub method: String,
    pub results: Vec<LatencyDto>,
}

pub fn latency_router() -> Router<AppState> {
    Router::new()
        .route("/latency", get(list_latency))
        .route("/latency/test", post(test_latency))
}

async fn list_latency(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ListLatencyResponse>, ApiError> {
    let rows = list_latency_results(&state.pool).await?;
    Ok(Json(ListLatencyResponse {
        results: rows.into_iter().map(LatencyDto::from).collect(),
    }))
}

async fn test_latency(
    _user: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<TestLatencyRequest>,
) -> Result<Json<TestLatencyResponse>, ApiError> {
    let nodes = select_nodes(&state, body.ids.as_ref()).await?;
    let tested_at = now_rfc3339();

    // Try real proxy probe via chaos-prober first.
    if let Some(ref prober_bin) = state.prober_bin {
        let targets: Vec<(String, String)> = nodes
            .iter()
            .map(|n| (n.id.clone(), n.link.clone()))
            .collect();

        match chaos_core::latency::probe_via_prober(prober_bin, targets, PROBE_TIMEOUT, &tested_at)
            .await
        {
            Ok(samples) => {
                let mut results = Vec::with_capacity(samples.len());
                for sample in samples {
                    upsert_latency_result(
                        &state.pool,
                        &sample.node_id,
                        sample.latency_ms.map(i64::from),
                        sample.alive,
                        &sample.tested_at,
                        sample.message.as_deref(),
                    )
                    .await?;
                    results.push(LatencyDto {
                        id: sample.node_id,
                        latency_ms: sample.latency_ms,
                        alive: sample.alive,
                        tested_at: sample.tested_at,
                        message: sample.message,
                    });
                }
                return Ok(Json(TestLatencyResponse {
                    method: "proxy".into(),
                    results,
                }));
            }
            Err(e) => {
                tracing::warn!(error = %e, "prober failed, falling back to TCP probe");
            }
        }
    }

    // Fallback: TCP connect probe.
    let mut targets = Vec::new();
    let mut skipped = Vec::new();

    for node in nodes {
        match resolve_probe_target(node.address.as_deref(), &node.link) {
            Some(addr) => targets.push((node.id, addr)),
            None => {
                skipped.push((node.id, "no host:port address for TCP probe".to_string()));
            }
        }
    }

    let samples = probe_batch(targets, PROBE_TIMEOUT, &tested_at).await;

    let mut results = Vec::with_capacity(samples.len() + skipped.len());

    for sample in samples {
        upsert_latency_result(
            &state.pool,
            &sample.node_id,
            sample.latency_ms.map(i64::from),
            sample.alive,
            &sample.tested_at,
            sample.message.as_deref(),
        )
        .await?;

        results.push(LatencyDto {
            id: sample.node_id,
            latency_ms: sample.latency_ms,
            alive: sample.alive,
            tested_at: sample.tested_at,
            message: sample.message,
        });
    }

    for (node_id, message) in skipped {
        upsert_latency_result(
            &state.pool,
            &node_id,
            None,
            false,
            &tested_at,
            Some(&message),
        )
        .await?;

        results.push(LatencyDto {
            id: node_id,
            latency_ms: None,
            alive: false,
            tested_at: tested_at.clone(),
            message: Some(message),
        });
    }

    Ok(Json(TestLatencyResponse {
        method: "tcp".into(),
        results,
    }))
}

async fn select_nodes(state: &AppState, ids: Option<&Vec<String>>) -> Result<Vec<Node>, ApiError> {
    let all = list_nodes(&state.pool).await?;
    match ids {
        None => Ok(all),
        Some(ids) if ids.is_empty() => Ok(Vec::new()),
        Some(ids) => {
            let set: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
            Ok(all
                .into_iter()
                .filter(|n| set.contains(n.id.as_str()))
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chaos_store::{connect, insert_node, migrate};
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
            .nest("/api/v1", latency_router())
            .with_state(state.clone());
        (app, state)
    }

    async fn json_body(res: axum::response::Response) -> serde_json::Value {
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn latency_requires_auth() {
        let (app, _) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/latency")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let (app, _) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/latency/test")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"ids":null}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_and_list_latency_localhost() {
        let (app, state) = test_app().await;
        let token = issue_token("u1", "admin", &state.jwt_secret).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                if listener.accept().await.is_err() {
                    break;
                }
            }
        });

        let node = insert_node(
            &state.pool,
            "local",
            None,
            &format!("trojan://x@127.0.0.1:{port}"),
            Some("trojan"),
            Some(&format!("127.0.0.1:{port}")),
            None,
        )
        .await
        .unwrap();

        let test = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/latency/test")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"ids":null}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(test.status(), StatusCode::OK);
        let body = json_body(test).await;
        assert_eq!(body["results"].as_array().unwrap().len(), 1);
        assert_eq!(body["results"][0]["id"], node.id);
        assert_eq!(body["results"][0]["alive"], true);
        assert!(body["results"][0]["latency_ms"].as_u64().unwrap() < 2000);

        let list = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/latency")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let listed = json_body(list).await;
        assert_eq!(listed["results"][0]["id"], node.id);
        assert_eq!(listed["results"][0]["alive"], true);
    }
}
