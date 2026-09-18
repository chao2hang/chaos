//! Group CRUD + membership with weights (auth required).

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::{AdminUser, AuthUser};
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::routes::orchestration::{
    mark_republish_if_published_source_changed, orchestration_references_source,
};
use crate::state::AppState;

const GROUP_POLICIES: [&str; 5] = ["min_moving_avg", "min", "min_avg10", "random", "fixed"];

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

fn normalized_group_name(name: &str, locale: chaos_i18n::Locale) -> Result<String, ApiError> {
    let normalized = chaos_core::config_render::normalized_dae_identifier(name)
        .ok_or_else(|| ApiError::bad_request("invalid_request", locale))?;
    if chaos_core::config_render::is_reserved_dae_identifier(&normalized) {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    Ok(normalized)
}

fn validate_group_fields(
    name: &str,
    policy: &str,
    filter_tag: Option<&str>,
    locale: chaos_i18n::Locale,
) -> Result<String, ApiError> {
    let normalized = normalized_group_name(name, locale)?;
    if !GROUP_POLICIES.contains(&policy) {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    if filter_tag.is_some_and(|tag| {
        tag.len() > 128
            || tag
                .chars()
                .any(|character| matches!(character, '\r' | '\n' | '(' | ')' | ','))
    }) {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    Ok(normalized)
}

async fn ensure_unique_group_name(
    state: &AppState,
    normalized_name: &str,
    exclude_id: Option<&str>,
    locale: chaos_i18n::Locale,
) -> Result<(), ApiError> {
    let collision = chaos_store::list_groups(&state.pool)
        .await?
        .into_iter()
        .any(|group| {
            exclude_id != Some(group.id.as_str())
                && chaos_core::config_render::normalized_dae_identifier(&group.name)
                    .is_some_and(|name| name.eq_ignore_ascii_case(normalized_name))
        });
    if collision {
        return Err(ApiError::conflict("invalid_request", locale));
    }
    Ok(())
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

/// Enrich `groups` with their members and the member nodes' display fields.
///
/// Everything is fetched in three queries regardless of how many groups are
/// passed: the node table once, and every membership once. The previous
/// per-group helper issued two queries per group — including a full re-read of
/// the node table for each one — so a console with N groups cost 2N queries.
async fn enrich_groups(
    state: &AppState,
    groups: Vec<chaos_store::Group>,
) -> Result<Vec<GroupDto>, ApiError> {
    let nodes = chaos_store::list_nodes(&state.pool).await?;
    let by_id: std::collections::HashMap<_, _> =
        nodes.into_iter().map(|n| (n.id.clone(), n)).collect();

    let mut members_by_group: std::collections::HashMap<String, Vec<chaos_store::GroupMember>> =
        std::collections::HashMap::new();
    for member in chaos_store::list_all_group_members(&state.pool).await? {
        members_by_group
            .entry(member.group_id.clone())
            .or_default()
            .push(member);
    }

    Ok(groups
        .into_iter()
        .map(|g| {
            let members = members_by_group
                .remove(&g.id)
                .unwrap_or_default()
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
            GroupDto {
                id: g.id,
                name: g.name,
                policy: g.policy,
                filter_tag: g.filter_tag,
                sort_order: g.sort_order,
                created_at: g.created_at,
                members,
            }
        })
        .collect())
}

async fn enrich_group(state: &AppState, g: chaos_store::Group) -> Result<GroupDto, ApiError> {
    let mut enriched = enrich_groups(state, vec![g]).await?;
    enriched
        .pop()
        .ok_or_else(|| ApiError::internal(chaos_i18n::Locale::En))
}

async fn list_groups(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let groups = chaos_store::list_groups(&state.pool).await?;
    let out = enrich_groups(&state, groups).await?;
    Ok(Json(serde_json::json!({ "groups": out })))
}

async fn create_group(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<CreateGroupRequest>,
) -> Result<Json<GroupDto>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    let name = body.name.trim();
    let policy = body.policy.trim();
    let tag = body
        .filter_tag
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let normalized = validate_group_fields(name, policy, tag, locale)?;
    ensure_unique_group_name(&state, &normalized, None, locale).await?;
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
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
    Json(body): Json<UpdateGroupRequest>,
) -> Result<Json<GroupDto>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    let name = body.name.trim();
    let policy = body.policy.trim();
    let tag = body
        .filter_tag
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let normalized = validate_group_fields(name, policy, tag, locale)?;
    ensure_unique_group_name(&state, &normalized, Some(&id), locale).await?;
    let ok = chaos_store::update_group(&state.pool, &id, name, policy, tag, body.sort_order)
        .await
        .map_err(|error| {
            let message = error.to_string();
            if message.contains("UNIQUE") || message.contains("unique") {
                ApiError::conflict("invalid_request", locale)
            } else {
                ApiError::from(error)
            }
        })?;
    if !ok {
        return Err(ApiError::not_found("not_found", locale));
    }
    let groups = chaos_store::list_groups(&state.pool).await?;
    let g = groups
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;
    let _ = mark_republish_if_published_source_changed(&state, "group", &id).await?;
    Ok(Json(enrich_group(&state, g).await?))
}

async fn delete_group(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    if orchestration_references_source(&state, "group", &id).await? {
        return Err(ApiError::conflict("resource_in_use", locale));
    }
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
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
    Json(body): Json<ReplaceMembersBody>,
) -> Result<Json<GroupDto>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    let groups = chaos_store::list_groups(&state.pool).await?;
    let g = groups
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| ApiError::not_found("not_found", locale))?;
    let known_nodes: std::collections::HashSet<String> = chaos_store::list_nodes(&state.pool)
        .await?
        .into_iter()
        .map(|node| node.id)
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut rows = Vec::with_capacity(body.members.len());
    for (index, member) in body.members.iter().enumerate() {
        let node_id = member.node_id.trim();
        if node_id.is_empty() || !known_nodes.contains(node_id) {
            return Err(ApiError::not_found("not_found", locale));
        }
        if !(1..=99).contains(&member.weight) || !seen.insert(node_id.to_string()) {
            return Err(ApiError::bad_request("invalid_request", locale));
        }
        rows.push((node_id.to_string(), member.weight, index as i64));
    }
    chaos_store::replace_group_members(&state.pool, &id, &rows).await?;
    let _ = mark_republish_if_published_source_changed(&state, "group", &id).await?;
    Ok(Json(enrich_group(&state, g).await?))
}

async fn add_member(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
    Json(body): Json<MemberBody>,
) -> Result<Json<GroupDto>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    let node_id = body.node_id.trim();
    if node_id.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    if !(1..=99).contains(&body.weight) {
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
    let _ = mark_republish_if_published_source_changed(&state, "group", &id).await?;
    Ok(Json(enrich_group(&state, g).await?))
}

async fn remove_member(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path((id, node_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    let deleted = chaos_store::remove_group_member(&state.pool, &id, &node_id).await?;
    if !deleted {
        return Err(ApiError::not_found("not_found", locale));
    }
    let _ = mark_republish_if_published_source_changed(&state, "group", &id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn patch_weight(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path((id, node_id)): Path<(String, String)>,
    Json(body): Json<WeightBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _runtime_guard = state.runtime_lock.lock().await;
    if !(1..=99).contains(&body.weight) {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let ok = chaos_store::set_member_weight(&state.pool, &id, &node_id, body.weight).await?;
    if !ok {
        return Err(ApiError::not_found("not_found", locale));
    }
    let _ = mark_republish_if_published_source_changed(&state, "group", &id).await?;
    Ok(Json(serde_json::json!({
        "node_id": node_id,
        "weight": body.weight,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chaos_store::{connect, insert_node, migrate};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::auth::{auth_router, issue_token, issue_token_role};

    async fn test_app() -> (Router, AppState) {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        // `AdminUser` reads the role from the users table rather than from the
        // token, so the non-admin case needs a real row to exist.
        for (id, username, role) in [("u1", "admin", "admin"), ("u2", "viewer", "user")] {
            sqlx::query(
                "INSERT INTO users (id, username, password_hash, created_at, role) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(username)
            .bind("test-hash")
            .bind("now")
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();
        }
        let state = AppState::new(pool, "test-secret-key-for-jwt-hs256".to_string());
        let app = Router::new()
            .nest("/api/v1/auth", auth_router())
            .nest("/api/v1", groups_router())
            .with_state(state.clone());
        (app, state)
    }

    async fn json_body(res: axum::response::Response) -> serde_json::Value {
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn authed(method: &str, uri: &str, token: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn non_admin_cannot_write_groups() {
        let (app, state) = test_app().await;
        let admin = issue_token("u1", "admin", &state.jwt_secret).unwrap();
        let viewer = issue_token_role("u2", "viewer", "user", &state.jwt_secret).unwrap();
        let group = chaos_store::insert_group(&state.pool, "g1", "min", None, 0)
            .await
            .unwrap();
        insert_node(
            &state.pool,
            "n1",
            None,
            "trojan://x@127.0.0.1:443",
            Some("trojan"),
            None,
            None,
        )
        .await
        .unwrap();
        let members = format!("/api/v1/groups/{}/members", group.id);

        let create = app
            .clone()
            .oneshot(authed(
                "POST",
                "/api/v1/groups",
                &viewer,
                r#"{"name":"g2","policy":"min"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(create.status(), StatusCode::FORBIDDEN);
        assert_eq!(json_body(create).await["error"]["code"], "admin_required");

        let update = app
            .clone()
            .oneshot(authed(
                "PATCH",
                &format!("/api/v1/groups/{}", group.id),
                &viewer,
                r#"{"name":"g2","policy":"min"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(update.status(), StatusCode::FORBIDDEN);

        let delete = app
            .clone()
            .oneshot(authed(
                "DELETE",
                &format!("/api/v1/groups/{}", group.id),
                &viewer,
                "",
            ))
            .await
            .unwrap();
        assert_eq!(delete.status(), StatusCode::FORBIDDEN);

        let replace = app
            .clone()
            .oneshot(authed(
                "PUT",
                &members,
                &viewer,
                r#"{"members":[{"node_id":"n1","weight":1}]}"#,
            ))
            .await
            .unwrap();
        assert_eq!(replace.status(), StatusCode::FORBIDDEN);

        let add = app
            .clone()
            .oneshot(authed(
                "POST",
                &members,
                &viewer,
                r#"{"node_id":"n1","weight":1}"#,
            ))
            .await
            .unwrap();
        assert_eq!(add.status(), StatusCode::FORBIDDEN);

        let weight = app
            .clone()
            .oneshot(authed(
                "PATCH",
                &format!("/api/v1/groups/{}/members/n1", group.id),
                &viewer,
                r#"{"weight":2}"#,
            ))
            .await
            .unwrap();
        assert_eq!(weight.status(), StatusCode::FORBIDDEN);

        // Reads stay open to any authenticated user, and the rejected writes
        // must not have touched stored state.
        let list = app
            .clone()
            .oneshot(authed("GET", "/api/v1/groups", &viewer, ""))
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);

        let list = app
            .oneshot(authed("GET", "/api/v1/groups", &admin, ""))
            .await
            .unwrap();
        let listed = json_body(list).await;
        let groups = listed["groups"].as_array().unwrap();
        // The migration seeds a default `proxy` group; the rejected writes must
        // not have added anything beyond it.
        assert!(
            !groups.iter().any(|group| group["name"] == "g2"),
            "a rejected write created a group: {listed}"
        );
        let g1 = groups
            .iter()
            .find(|group| group["name"] == "g1")
            .expect("seeded group is still listed");
        assert_eq!(g1["members"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn members_are_enriched_from_the_node_table() {
        let (app, state) = test_app().await;
        let admin = issue_token("u1", "admin", &state.jwt_secret).unwrap();
        let node = insert_node(
            &state.pool,
            "n1",
            Some("hk"),
            "trojan://x@127.0.0.1:443",
            Some("trojan"),
            Some("127.0.0.1:443"),
            None,
        )
        .await
        .unwrap();

        let created = app
            .clone()
            .oneshot(authed(
                "POST",
                "/api/v1/groups",
                &admin,
                r#"{"name":"g1","policy":"min"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::OK);
        let created = json_body(created).await;
        let id = created["id"].as_str().unwrap().to_string();
        assert_eq!(created["policy"], "min");
        assert!(created["members"].as_array().unwrap().is_empty());

        let added = app
            .clone()
            .oneshot(authed(
                "POST",
                &format!("/api/v1/groups/{id}/members"),
                &admin,
                &format!(r#"{{"node_id":"{}","weight":3}}"#, node.id),
            ))
            .await
            .unwrap();
        assert_eq!(added.status(), StatusCode::OK);

        // The list route batches enrichment; its members must carry the display
        // fields of the referenced node.
        let listed = json_body(
            app.clone()
                .oneshot(authed("GET", "/api/v1/groups", &admin, ""))
                .await
                .unwrap(),
        )
        .await;
        let groups = listed["groups"].as_array().unwrap();
        let group = groups
            .iter()
            .find(|group| group["id"] == id.as_str())
            .expect("created group is listed");
        let members = group["members"].as_array().unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0]["name"], "n1");
        assert_eq!(members[0]["tag"], "hk");
        assert_eq!(members[0]["weight"], 3);

        let removed = app
            .oneshot(authed(
                "DELETE",
                &format!("/api/v1/groups/{id}/members/{}", node.id),
                &admin,
                "",
            ))
            .await
            .unwrap();
        assert_eq!(removed.status(), StatusCode::OK);
    }
}
