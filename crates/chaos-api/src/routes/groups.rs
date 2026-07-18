//! Group CRUD (auth required).

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct GroupDto {
    pub id: String,
    pub name: String,
    pub policy: String,
    pub filter_tag: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
}

impl From<chaos_store::Group> for GroupDto {
    fn from(g: chaos_store::Group) -> Self {
        Self {
            id: g.id,
            name: g.name,
            policy: g.policy,
            filter_tag: g.filter_tag,
            sort_order: g.sort_order,
            created_at: g.created_at,
        }
    }
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

pub fn groups_router() -> Router<AppState> {
    Router::new()
        .route("/groups", get(list_groups).post(create_group))
        .route(
            "/groups/{id}",
            axum::routing::patch(update_group).delete(delete_group),
        )
}

async fn list_groups(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let groups = chaos_store::list_groups(&state.pool).await?;
    Ok(Json(serde_json::json!({
        "groups": groups.into_iter().map(GroupDto::from).collect::<Vec<_>>(),
    })))
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
    Ok(Json(GroupDto::from(g)))
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
    Ok(Json(GroupDto::from(g)))
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
