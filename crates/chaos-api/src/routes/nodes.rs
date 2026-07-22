//! Node list / import / update / delete routes (auth required).

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use chaos_i18n::error_message;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use chaos_store::{
    delete_node, get_node, insert_node_with_id, list_nodes, update_node, NewNode, Node, UpdateNode,
};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::routes::orchestration::{
    active_plan_references_any_node, mark_republish_if_published_node_added,
    mark_republish_if_published_source_changed, orchestration_references_source,
};
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
    Ok {
        ok: bool,
        node: NodeDto,
    },
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

#[derive(Debug, Deserialize)]
pub struct UpdateNodeRequest {
    /// Optional display name override. When omitted (or blank), name is derived from link tag / protocol.
    pub name: Option<String>,
    /// Optional tag. When omitted, derived from the link fragment when present.
    pub tag: Option<String>,
    /// Replacement share link (required).
    pub link: String,
}

pub fn nodes_router() -> Router<AppState> {
    Router::new()
        .route("/nodes", get(list_nodes_handler).post(import_nodes))
        .route(
            "/nodes/{id}",
            axum::routing::patch(update_node_handler).delete(delete_node_handler),
        )
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
    RequestLocale(locale): RequestLocale,
    Json(body): Json<ImportNodesRequest>,
) -> Result<Json<ImportNodesResponse>, ApiError> {
    if body.links.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }

    let _runtime_guard = state.runtime_lock.lock().await;

    let mut results = Vec::with_capacity(body.links.len());

    for item in body.links {
        let raw = item.link.trim();
        if raw.is_empty() {
            results.push(ImportItemResult::Err {
                ok: false,
                link: item.link,
                error: ImportErrorBody {
                    code: "link_required",
                    message: error_message(locale, "link_required"),
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
                    code: "unrecognized_scheme",
                    message: error_message(locale, "unrecognized_scheme"),
                },
            });
            continue;
        }

        let address = chaos_core::link::detect_address(raw);
        // Manual import: user-supplied tag wins, else extract from link fragment.
        let tag_owned: Option<String> = item
            .tag
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|s| s.to_string())
            .or_else(|| chaos_core::link::detect_tag(raw));

        let id = Uuid::new_v4().to_string();
        // No subscription-level tag for manual imports.
        let name =
            chaos_core::link::node_name(tag_owned.as_deref(), None, protocol.as_deref(), &id);

        match insert_node_with_id(
            &state.pool,
            Some(id),
            NewNode {
                name: &name,
                tag: tag_owned.as_deref(),
                link: raw,
                protocol: protocol.as_deref(),
                address: address.as_deref(),
                subscription_id: None,
            },
        )
        .await
        {
            Ok(node) => {
                let _ = mark_republish_if_published_node_added(
                    &state,
                    &node.id,
                    node.subscription_id.as_deref(),
                    node.tag.as_deref(),
                )
                .await?;
                results.push(ImportItemResult::Ok {
                    ok: true,
                    node: NodeDto::from(node),
                });
            }
            Err(e) => {
                tracing::error!(?e, "insert node failed");
                results.push(ImportItemResult::Err {
                    ok: false,
                    link: raw.to_string(),
                    error: ImportErrorBody {
                        code: "database_error",
                        message: error_message(locale, "database_error"),
                    },
                });
            }
        }
    }

    // GeoIP: look up country codes for successfully imported nodes.
    let geo_pairs: Vec<(String, String)> = results
        .iter()
        .filter_map(|r| match r {
            ImportItemResult::Ok { node, .. } => {
                node.address.as_ref().map(|a| (node.id.clone(), a.clone()))
            }
            _ => None,
        })
        .collect();

    drop(_runtime_guard);

    if chaos_core::geoip::enabled() && !geo_pairs.is_empty() {
        let geo = chaos_core::geoip::batch_lookup_country(&geo_pairs).await;
        for (node_id, cc) in &geo {
            let _ = chaos_store::update_node_country_code(&state.pool, node_id, cc).await;
        }
        // Patch country_code into response DTOs.
        for r in &mut results {
            if let ImportItemResult::Ok { node, .. } = r {
                if let Some(cc) = geo.get(&node.id) {
                    node.country_code = Some(cc.clone());
                }
            }
        }
    }

    Ok(Json(ImportNodesResponse { results }))
}

async fn update_node_handler(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
    Json(body): Json<UpdateNodeRequest>,
) -> Result<Json<NodeDto>, ApiError> {
    let raw = body.link.trim();
    if raw.is_empty() {
        return Err(ApiError::bad_request("link_required", locale));
    }

    let protocol = chaos_core::link::detect_protocol(raw).ok_or_else(|| {
        ApiError::bad_request("unrecognized_scheme", locale)
    })?;
    let address = chaos_core::link::detect_address(raw);

    let existing = get_node(&state.pool, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;

    let explicit_name = body
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let explicit_tag = body
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Prefer explicit tag, else link fragment, else keep previous tag.
    let tag_owned: Option<String> = explicit_tag
        .or_else(|| chaos_core::link::detect_tag(raw))
        .or_else(|| existing.tag.clone());

    let name = if let Some(name) = explicit_name {
        name
    } else {
        chaos_core::link::node_name(
            tag_owned.as_deref(),
            None,
            Some(protocol.as_str()),
            &id,
        )
    };

    let address_changed = existing.address.as_deref() != address.as_deref();
    let link_changed = existing.link != raw;

    let _runtime_guard = state.runtime_lock.lock().await;

    let node = update_node(
        &state.pool,
        &id,
        UpdateNode {
            name: &name,
            tag: tag_owned.as_deref(),
            link: raw,
            protocol: Some(protocol.as_str()),
            address: address.as_deref(),
            clear_country_code: address_changed,
        },
    )
    .await?
    .ok_or_else(|| ApiError::not_found("not_found", locale))?;

    let mut dto = NodeDto::from(node);

    if link_changed {
        let _ = mark_republish_if_published_source_changed(&state, "node", &id).await?;
        let _ = mark_republish_if_published_node_added(
            &state,
            &dto.id,
            dto.subscription_id.as_deref(),
            dto.tag.as_deref(),
        )
        .await?;
    }

    drop(_runtime_guard);

    if address_changed && chaos_core::geoip::enabled() {
        if let Some(addr) = dto.address.clone() {
            let geo = chaos_core::geoip::batch_lookup_country(&[(dto.id.clone(), addr)]).await;
            if let Some(cc) = geo.get(&dto.id) {
                let _ = chaos_store::update_node_country_code(&state.pool, &dto.id, cc).await;
                dto.country_code = Some(cc.clone());
            }
        }
    }

    Ok(Json(dto))
}

async fn delete_node_handler(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> Result<Json<DeleteNodeResponse>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    if orchestration_references_source(&state, "node", &id).await?
        || active_plan_references_any_node(&state, &std::iter::once(id.clone()).collect()).await?
    {
        return Err(ApiError::conflict("resource_in_use", locale));
    }
    let deleted = delete_node(&state.pool, &id).await?;
    if !deleted {
        return Err(ApiError::not_found("not_found", locale));
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
    async fn import_list_update_delete_nodes() {
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
        let id = body["results"][0]["node"]["id"]
            .as_str()
            .unwrap()
            .to_string();

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

        let patch = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/nodes/{id}"))
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name":"renamed","tag":"sg","link":"hysteria2://u@9.9.9.9:8443#sg"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(patch.status(), StatusCode::OK);
        let patched = json_body(patch).await;
        assert_eq!(patched["id"], id);
        assert_eq!(patched["name"], "renamed");
        assert_eq!(patched["tag"], "sg");
        assert_eq!(patched["protocol"], "hysteria2");
        assert_eq!(patched["address"], "9.9.9.9:8443");
        assert_eq!(patched["link"], "hysteria2://u@9.9.9.9:8443#sg");

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

    #[tokio::test]
    async fn import_invalid_link_localizes() {
        let (app, state) = test_app().await;
        let token = issue_token("u1", "admin", &state.jwt_secret).unwrap();
        let import = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/nodes")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .header("accept-language", "zh-CN")
                    .body(Body::from(
                        r#"{"links":[{"link":"not-a-valid-share-link"}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(import.status(), StatusCode::OK);
        let body = json_body(import).await;
        assert_eq!(body["results"][0]["ok"], false);
        assert_eq!(body["results"][0]["error"]["code"], "unrecognized_scheme");
        assert_eq!(
            body["results"][0]["error"]["message"],
            "无法识别分享链接协议"
        );
    }
}
