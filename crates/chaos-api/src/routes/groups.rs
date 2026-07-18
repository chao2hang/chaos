//! Group CRUD + membership with weights (auth required).

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct GroupMemberDto {
    pub node_id: String,
    pub weight: i64,
    pub sort_order: i64,
    pub name: Option<String>,
    pub tag: Option<String>,
    pub protocol: Option<String>,
    pub address: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GroupDto {
    pub id: String,
    pub name: String,
    pub policy: String,
    pub filter_tag: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub members: Vec<GroupMemberDto>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    #[serde(default = "default_policy")]
    pub policy: String,
    pub filter_tag: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
}

fn default_policy() -> String {
    "min_moving_avg".into()
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupRequest {
    pub name: String,
    pub policy: String,
    pub filter_tag: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
}

#[derive(Debug, Deserialize)]
pub struct MemberBody {
    pub node_id: String,
    #[serde(default = "default_weight")]
    pub weight: i64,
}

fn default_weight() -> i64 {
    1
}

#[derive(Debug, Deserialize)]
pub struct ReplaceMembersBody {
    pub members: Vec<MemberBody>,
}

#[derive(Debug, Deserialize)]
pub struct WeightBody {
    pub weight: i64,
}

pub fn groups_router() -> Router<AppState> {
    Router::new()
        .route("/groups", get(list_groups).post(create_group))
        .route(
            "/groups/{id}",
            axum::routing::patch(update_group).delete(delete_group),
        )
        .route(
            "/groups/{id}/members",
            get(list_members).put(replace_members).post(add_member),
        )
        .route(
            "/groups/{id}/members/{node_id}",
            axum::routing::delete(remove_member).patch(patch_weight),
        )
}

async fn enrich_group(state: &AppState, g: chaos_store::Group) -> Result<GroupDto, ApiError> {
    let members = chaos_store::list_group_members(&state.pool, &g.id).await?;
    let nodes = chaos_store::list_nodes(&state.pool).await?;
    let by_id: std::collections::HashMap<_, _> =
        nodes.into_iter().map(|n| (n.id.clone(), n)).collect();
    let members = members
        .into_iter()
        .map(|m| {
            let n = by_id.get(&m.node_id);
            GroupMemberDto {
                node_id: m.node_id,
                weight: m.weight,
                sort_order: m.sort_order,
                name: n.map(|x| x.name.clone()),
                tag: n.and_then(|x| x.tag.clone()),
                protocol: n.and_then(|x| x.protocol.clone()),
                address: n.and_then(|x| x.address.clone()),
            }
        })
        .collect();
    Ok(GroupDto {
        id: g.id,
        name: g.name,
        policy: g.policy,
        filter_tag: g.filter_tag,
        sort_order: g.sort_order,
        created_at: g.created_at,
        members,
    })
}

async fn list_groups(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let groups = chaos_store::list_groups(&state.pool).await?;
    let mut out = Vec::with_capacity(groups.len());
    for g in groups {
        out.push(enrich_group(&state, g).await?);
    }
    Ok(Json(serde_json::json!({ "groups": out })))
}

async fn create_group(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<CreateGroupRequest>,
) -> Result<Json<GroupDto>, ApiError> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let policy = body.policy.trim();
    if policy.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let tag = body
        .filter_tag
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let g = chaos_store::insert_group(&state.pool, name, policy, tag, body.sort_order)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("UNIQUE") || msg.contains("unique") {
                ApiError::conflict("invalid_request", locale)
            } else {
                ApiError::from(e)
            }
        })?;
    Ok(Json(enrich_group(&state, g).await?))
}

async fn update_group(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
    Json(body): Json<UpdateGroupRequest>,
) -> Result<Json<GroupDto>, ApiError> {
    let name = body.name.trim();
    let policy = body.policy.trim();
    if name.is_empty() || policy.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let tag = body
        .filter_tag
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let ok = chaos_store::update_group(&state.pool, &id, name, policy, tag, body.sort_order).await?;
    if !ok {
        return Err(ApiError::not_found("not_found", locale));
    }
    let groups = chaos_store::list_groups(&state.pool).await?;
    let g = groups
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;
    Ok(Json(enrich_group(&state, g).await?))
}

async fn delete_group(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = chaos_store::delete_group(&state.pool, &id).await?;
    if !deleted {
        return Err(ApiError::not_found("not_found", locale));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn list_members(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let groups = chaos_store::list_groups(&state.pool).await?;
    if !groups.iter().any(|g| g.id == id) {
        return Err(ApiError::not_found("not_found", locale));
    }
    let g = groups.into_iter().find(|g| g.id == id).unwrap();
    let dto = enrich_group(&state, g).await?;
    Ok(Json(serde_json::json!({ "members": dto.members })))
}

async fn replace_members(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
    Json(body): Json<ReplaceMembersBody>,
) -> Result<Json<GroupDto>, ApiError> {
    let groups = chaos_store::list_groups(&state.pool).await?;
    let g = groups
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;
    let rows: Vec<(String, i64, i64)> = body
        .members
        .iter()
        .enumerate()
        .map(|(i, m)| (m.node_id.trim().to_string(), m.weight.max(1), i as i64))
        .filter(|(nid, _, _)| !nid.is_empty())
        .collect();
    chaos_store::replace_group_members(&state.pool, &id, &rows).await?;
    Ok(Json(enrich_group(&state, g).await?))
}

async fn add_member(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
    Json(body): Json<MemberBody>,
) -> Result<Json<GroupDto>, ApiError> {
    let node_id = body.node_id.trim();
    if node_id.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let groups = chaos_store::list_groups(&state.pool).await?;
    let g = groups
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;
    if chaos_store::get_node(&state.pool, node_id).await?.is_none() {
        return Err(ApiError::not_found("not_found", locale));
    }
    chaos_store::add_group_member(&state.pool, &id, node_id, body.weight).await?;
    Ok(Json(enrich_group(&state, g).await?))
}

async fn remove_member(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path((id, node_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = chaos_store::remove_group_member(&state.pool, &id, &node_id).await?;
    if !deleted {
        return Err(ApiError::not_found("not_found", locale));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn patch_weight(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path((id, node_id)): Path<(String, String)>,
    Json(body): Json<WeightBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ok = chaos_store::set_member_weight(&state.pool, &id, &node_id, body.weight).await?;
    if !ok {
        return Err(ApiError::not_found("not_found", locale));
    }
    Ok(Json(serde_json::json!({
        "node_id": node_id,
        "weight": body.weight.max(1),
    })))
}
