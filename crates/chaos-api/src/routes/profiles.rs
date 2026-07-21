//! Config profiles: list / create / update / delete / activate.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ProfileDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<chaos_store::ConfigProfile> for ProfileDto {
    fn from(p: chaos_store::ConfigProfile) -> Self {
        Self {
            id: p.id,
            name: p.name,
            description: p.description,
            is_active: p.is_active != 0,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListProfilesResponse {
    pub profiles: Vec<ProfileDto>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProfileRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// OrchestrationDocument JSON. If omitted, snapshots the current draft.
    pub document: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateProfileResponse {
    pub profile: ProfileDto,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub document: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateProfileResponse {
    pub profile: ProfileDto,
}

#[derive(Debug, Serialize)]
pub struct DeleteProfileResponse {
    pub deleted: bool,
}

#[derive(Debug, Serialize)]
pub struct ActivateProfileResponse {
    pub profile: ProfileDto,
}

async fn list_profiles(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ListProfilesResponse>, ApiError> {
    let profiles = chaos_store::list_profiles(&state.pool).await?;
    Ok(Json(ListProfilesResponse {
        profiles: profiles.into_iter().map(ProfileDto::from).collect(),
    }))
}

async fn create_profile(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<CreateProfileRequest>,
) -> Result<Json<CreateProfileResponse>, ApiError> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("name_required", locale));
    }

    // Use provided document or snapshot current orchestration draft.
    let document = match body.document {
        Some(doc) => doc,
        None => {
            chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_FLOW)
                .await?
                .unwrap_or_else(|| "{}".to_string())
        }
    };

    let profile = chaos_store::create_profile(&state.pool, name, &body.description, &document)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                ApiError::conflict("profile_name_exists", locale)
            } else {
                ApiError::internal_logged(locale, e)
            }
        })?;

    Ok(Json(CreateProfileResponse {
        profile: ProfileDto::from(profile),
    }))
}

async fn update_profile(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<UpdateProfileResponse>, ApiError> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("name_required", locale));
    }

    // Get existing profile to preserve document if not provided.
    let existing = chaos_store::get_profile(&state.pool, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;

    let document = body.document.unwrap_or(existing.document);

    let profile = chaos_store::update_profile(&state.pool, &id, name, &body.description, &document)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                ApiError::conflict("profile_name_exists", locale)
            } else {
                ApiError::internal_logged(locale, e)
            }
        })?
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;

    Ok(Json(UpdateProfileResponse {
        profile: ProfileDto::from(profile),
    }))
}

async fn delete_profile(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> Result<Json<DeleteProfileResponse>, ApiError> {
    let deleted = chaos_store::delete_profile(&state.pool, &id)
        .await?;
    if !deleted {
        return Err(ApiError::not_found("not_found", locale));
    }
    Ok(Json(DeleteProfileResponse { deleted: true }))
}

async fn activate_profile(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> Result<Json<ActivateProfileResponse>, ApiError> {
    let profile = chaos_store::activate_profile(&state.pool, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;

    // Also update the orchestration draft to match the activated profile.
    let _ = chaos_store::set_meta(
        &state.pool,
        chaos_store::META_ORCHESTRATION_FLOW,
        &profile.document,
    )
    .await;

    Ok(Json(ActivateProfileResponse {
        profile: ProfileDto::from(profile),
    }))
}

pub fn profiles_router() -> Router<AppState> {
    Router::new()
        .route("/profiles", get(list_profiles).post(create_profile))
        .route(
            "/profiles/{id}",
            get(|user, state, locale, Path(id): Path<String>| async move {
                get_profile(user, state, locale, Path(id)).await
            })
            .put(update_profile)
            .delete(delete_profile),
        )
        .route("/profiles/{id}/activate", post(activate_profile))
}

async fn get_profile(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> Result<Json<ProfileDto>, ApiError> {
    let profile = chaos_store::get_profile(&state.pool, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;
    Ok(Json(ProfileDto::from(profile)))
}
