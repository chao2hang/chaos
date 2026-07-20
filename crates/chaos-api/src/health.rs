//! Public health endpoint.

use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::state::AppState;

const API_VERSION: &str = "0.1.0";

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub api_version: &'static str,
    pub dae_binary: Option<String>,
    pub dae_binary_ok: bool,
    pub data_plane: &'static str,
    pub data_plane_ready: bool,
}

pub fn health_router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<HealthResponse> {
    let dae_path = if cfg!(windows) {
        None
    } else {
        chaos_dae::resolve_dae_bin()
    };
    let dae_binary_ok = dae_path
        .as_ref()
        .map(|p| chaos_dae::dae_bin_ok(p))
        .unwrap_or(false);
    let dae_binary = dae_path.map(|p| p.display().to_string());
    let data_plane = chaos_dae::platform_backend().status();

    Json(HealthResponse {
        ok: true,
        api_version: API_VERSION,
        dae_binary,
        dae_binary_ok,
        data_plane: data_plane.kind,
        data_plane_ready: data_plane.ready,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chaos_store::{connect, migrate};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn test_app() -> Router {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let state = AppState::new(pool, "test-secret".to_string());
        Router::new()
            .nest("/api/v1", health_router())
            .with_state(state)
    }

    #[tokio::test]
    async fn health_is_public_and_ok() {
        let app = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["api_version"], "0.1.0");
        assert!(body.get("dae_binary").is_some());
        assert!(body
            .get("dae_binary_ok")
            .and_then(|v| v.as_bool())
            .is_some());
    }
}
