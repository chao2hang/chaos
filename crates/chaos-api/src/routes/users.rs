//! User management routes (admin only).

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::{hash_password_locale, AdminUser};
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct UserDto {
    pub id: String,
    pub username: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ListUsersResponse {
    pub users: Vec<UserDto>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "user".into()
}

#[derive(Debug, Serialize)]
pub struct CreateUserResponse {
    pub user: UserDto,
}

#[derive(Debug, Serialize)]
pub struct DeleteUserResponse {
    pub deleted: bool,
}

async fn list_users(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<ListUsersResponse>, ApiError> {
    let users = chaos_store::users::list_users(&state.pool).await?;
    Ok(Json(ListUsersResponse {
        users: users
            .into_iter()
            .map(|u| UserDto {
                id: u.id,
                username: u.username,
                role: u.role,
                created_at: u.created_at,
            })
            .collect(),
    }))
}

async fn create_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<CreateUserRequest>,
) -> Result<Json<CreateUserResponse>, ApiError> {
    let username = body.username.trim();
    if username.is_empty() || username.len() > 128 {
        return Err(ApiError::bad_request("invalid_username", locale));
    }
    if body.password.len() < 8 || body.password.len() > 256 {
        return Err(ApiError::bad_request("invalid_password", locale));
    }
    let role = match body.role.as_str() {
        "admin" | "user" => body.role.clone(),
        _ => return Err(ApiError::bad_request("invalid_role", locale)),
    };

    // Check if username exists
    if chaos_store::find_user_by_username(&state.pool, username)
        .await?
        .is_some()
    {
        return Err(ApiError::conflict("username_exists", locale));
    }

    let hash = hash_password_locale(&body.password, locale)?;
    let user = chaos_store::users::create_user_with_role(&state.pool, username, &hash, &role)
        .await?;

    Ok(Json(CreateUserResponse {
        user: UserDto {
            id: user.id,
            username: user.username,
            role: user.role,
            created_at: user.created_at,
        },
    }))
}

async fn delete_user(
    admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> Result<Json<DeleteUserResponse>, ApiError> {
    // Cannot delete yourself
    if id == admin.0.user_id {
        return Err(ApiError::bad_request("cannot_delete_self", locale));
    }

    let deleted = chaos_store::users::delete_user(&state.pool, &id).await?;
    if !deleted {
        return Err(ApiError::not_found("not_found", locale));
    }
    Ok(Json(DeleteUserResponse { deleted: true }))
}

pub fn users_router() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", axum::routing::delete(delete_user))
}
