//! Node list / import / delete routes (auth required).

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use chaos_store::{delete_node, insert_node_with_id, list_nodes, Node};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

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
pub struct ListNodesResponse {
    pub nodes: Vec<NodeDto>,
}

#[derive(Debug, Deserialize)]
pub struct ImportNodesRequest {
    pub links: Vec<ImportLink>,
}

#[derive(Debug, Deserialize)]
pub struct ImportLink {
    pub link: String,
    pub tag: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportNodesResponse {
    pub results: Vec<ImportItemResult>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ImportItemResult {
    Ok { ok: bool, node: NodeDto },
    Err {
        ok: bool,
        link: String,
        error: ImportErrorBody,
    },
}

#[derive(Debug, Serialize)]
pub struct ImportErrorBody {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteNodeResponse {
    pub deleted: bool,
}

pub fn nodes_router() -> Router<AppState> {
    Router::new()
        .route("/nodes", get(list_nodes_handler).post(import_nodes))
        .route("/nodes/{id}", axum::routing::delete(delete_node_handler))
}

async fn list_nodes_handler(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ListNodesResponse>, ApiError> {
    let nodes = list_nodes(&state.pool).await?;
    Ok(Json(ListNodesResponse {
        nodes: nodes.into_iter().map(NodeDto::from).collect(),
    }))
}

async fn import_nodes(
    _user: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<ImportNodesRequest>,
) -> Result<Json<ImportNodesResponse>, ApiError> {
    if body.links.is_empty() {
        return Err(ApiError::bad_request(
            "invalid_request",
            "links must not be empty",
        ));
    }

    let mut results = Vec::with_capacity(body.links.len());

    for item in body.links {
        let raw = item.link.trim();
        if raw.is_empty() {
            results.push(ImportItemResult::Err {
                ok: false,
                link: item.link,
                error: ImportErrorBody {
                    code: "invalid_link",
                    message: "link is required".to_string(),
                },
            });
            continue;
        }

        let protocol = chaos_core::link::detect_protocol(raw);
        // Require a recognizable scheme for manual imports (MVP).
        if protocol.is_none() {
            results.push(ImportItemResult::Err {
                ok: false,
                link: raw.to_string(),
                error: ImportErrorBody {
                    code: "invalid_link",
                    message: "unrecognized share link scheme".to_string(),
                },
            });
            continue;
        }

        let address = chaos_core::link::detect_address(raw);
        let tag = item
            .tag
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());

        let id = Uuid::new_v4().to_string();
        let name = chaos_core::link::node_name(tag, protocol.as_deref(), &id);

        match insert_node_with_id(
            &state.pool,
            Some(id),
            &name,
            tag,
            raw,
            protocol.as_deref(),
            address.as_deref(),
            None,
        )
        .await
        {
            Ok(node) => results.push(ImportItemResult::Ok {
                ok: true,
                node: NodeDto::from(node),
            }),
            Err(e) => {
                tracing::error!(?e, "insert node failed");
                results.push(ImportItemResult::Err {
                    ok: false,
                    link: raw.to_string(),
                    error: ImportErrorBody {
                        code: "internal_error",
                        message: "database error".to_string(),
                    },
                });
            }
        }
    }

    Ok(Json(ImportNodesResponse { results }))
}

async fn delete_node_handler(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeleteNodeResponse>, ApiError> {
    let deleted = delete_node(&state.pool, &id).await?;
    if !deleted {
        return Err(ApiError::not_found("not_found", "node not found"));
    }
    Ok(Json(DeleteNodeResponse { deleted: true }))
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
            .nest("/api/v1", nodes_router())
            .with_state(state.clone());
        (app, state)
    }

    async fn json_body(res: axum::response::Response) -> serde_json::Value {
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn nodes_require_auth() {
        let (app, _) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/nodes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn import_list_delete_nodes() {
        let (app, state) = test_app().await;
        let token = issue_token("u1", "admin", &state.jwt_secret).unwrap();

        let import = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/nodes")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"links":[{"link":"trojan://example@1.2.3.4:443?sni=x#test","tag":"n1"}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(import.status(), StatusCode::OK);
        let body = json_body(import).await;
        assert_eq!(body["results"][0]["ok"], true);
        assert_eq!(body["results"][0]["node"]["name"], "n1");
        assert_eq!(body["results"][0]["node"]["protocol"], "trojan");
        assert_eq!(body["results"][0]["node"]["address"], "1.2.3.4:443");
        let id = body["results"][0]["node"]["id"].as_str().unwrap().to_string();

        let list = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/nodes")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let listed = json_body(list).await;
        assert_eq!(listed["nodes"].as_array().unwrap().len(), 1);

        let del = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/nodes/{id}"))
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
