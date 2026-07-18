//! DNS document GET/PUT (auth required).

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct DnsUpstreamDto {
    pub name: String,
    pub address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DnsRuleDto {
    pub expression: String,
    pub upstream: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DnsDocument {
    pub upstreams: Vec<DnsUpstreamDto>,
    pub rules: Vec<DnsRuleDto>,
    pub fallback: String,
}

pub fn dns_router() -> Router<AppState> {
    Router::new().route("/dns", get(get_dns).put(put_dns))
}

async fn get_dns(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<DnsDocument>, ApiError> {
    let upstreams = chaos_store::list_dns_upstreams(&state.pool).await?;
    let rules = chaos_store::list_dns_rules(&state.pool).await?;
    let fallback = chaos_store::get_meta(&state.pool, chaos_store::META_DNS_FALLBACK)
        .await?
        .unwrap_or_else(|| "alidns".into());
    Ok(Json(DnsDocument {
        upstreams: upstreams
            .into_iter()
            .map(|u| DnsUpstreamDto {
                name: u.name,
                address: u.address,
            })
            .collect(),
        rules: rules
            .into_iter()
            .map(|r| DnsRuleDto {
                expression: r.expression,
                upstream: r.upstream,
                enabled: r.enabled != 0,
            })
            .collect(),
        fallback,
    }))
}

async fn put_dns(
    _user: AuthUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<DnsDocument>,
) -> Result<Json<DnsDocument>, ApiError> {
    let fallback = body.fallback.trim();
    if fallback.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    if body.upstreams.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let ups: Vec<(String, String, i64)> = body
        .upstreams
        .iter()
        .enumerate()
        .map(|(i, u)| {
            (
                u.name.trim().to_string(),
                u.address.trim().to_string(),
                i as i64,
            )
        })
        .filter(|(n, a, _)| !n.is_empty() && !a.is_empty())
        .collect();
    if ups.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let rules: Vec<(String, String, i64, bool)> = body
        .rules
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (
                r.expression.trim().to_string(),
                r.upstream.trim().to_string(),
                i as i64,
                r.enabled,
            )
        })
        .filter(|(e, u, _, _)| !e.is_empty() && !u.is_empty())
        .collect();
    let (saved_ups, saved_rules) =
        chaos_store::replace_dns(&state.pool, &ups, &rules, fallback).await?;
    Ok(Json(DnsDocument {
        upstreams: saved_ups
            .into_iter()
            .map(|u| DnsUpstreamDto {
                name: u.name,
                address: u.address,
            })
            .collect(),
        rules: saved_rules
            .into_iter()
            .map(|r| DnsRuleDto {
                expression: r.expression,
                upstream: r.upstream,
                enabled: r.enabled != 0,
            })
            .collect(),
        fallback: fallback.to_string(),
    }))
}
