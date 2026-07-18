//! Routing document GET/PUT (auth required).

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct RoutingRuleDto {
    pub expression: String,
    pub outbound: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RoutingDocument {
    pub rules: Vec<RoutingRuleDto>,
    pub fallback: String,
}

pub fn routing_router() -> Router<AppState> {
    Router::new().route("/routing", get(get_routing).put(put_routing))
}

async fn get_routing(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<RoutingDocument>, ApiError> {
    let rules = chaos_store::list_routing_rules(&state.pool).await?;
    let fallback = chaos_store::get_meta(&state.pool, chaos_store::META_ROUTING_FALLBACK)
        .await?
        .unwrap_or_else(|| "proxy".into());
    Ok(Json(RoutingDocument {
        rules: rules
            .into_iter()
            .map(|r| RoutingRuleDto {
                expression: r.expression,
                outbound: r.outbound,
                enabled: r.enabled != 0,
            })
            .collect(),
        fallback,
    }))
}

async fn put_routing(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<RoutingDocument>,
) -> Result<Json<RoutingDocument>, ApiError> {
    let fallback = body.fallback.trim();
    if fallback.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let rows: Vec<(String, String, i64, bool)> = body
        .rules
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (
                r.expression.trim().to_string(),
                r.outbound.trim().to_string(),
                i as i64,
                r.enabled,
            )
        })
        .filter(|(e, o, _, _)| !e.is_empty() && !o.is_empty())
        .collect();
    let saved = chaos_store::replace_routing_rules(&state.pool, &rows, fallback).await?;
    Ok(Json(RoutingDocument {
        rules: saved
            .into_iter()
            .map(|r| RoutingRuleDto {
                expression: r.expression,
                outbound: r.outbound,
                enabled: r.enabled != 0,
            })
            .collect(),
        fallback: fallback.to_string(),
    }))
}
