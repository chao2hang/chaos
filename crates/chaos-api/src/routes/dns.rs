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
    let _runtime_guard = state.runtime_lock.lock().await;
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
    if body.upstreams.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let mut upstream_names = std::collections::HashMap::<String, String>::new();
    let mut ups = Vec::with_capacity(body.upstreams.len());
    for (index, upstream) in body.upstreams.iter().enumerate() {
        let raw_name = upstream.name.trim();
        let name = chaos_core::config_render::normalized_dae_identifier(raw_name)
            .ok_or_else(|| ApiError::bad_request("invalid_request", locale))?;
        let key = name.to_ascii_lowercase();
        if upstream_names.insert(key, name.clone()).is_some() {
            return Err(ApiError::conflict("invalid_request", locale));
        }
        let address = upstream.address.trim();
        if address.is_empty()
            || address.len() > 2048
            || address
                .chars()
                .any(|character| matches!(character, '\r' | '\n'))
        {
            return Err(ApiError::bad_request("invalid_request", locale));
        }
        ups.push((name, address.to_string(), index as i64));
    }

    let fallback = chaos_core::config_render::normalized_dae_identifier(body.fallback.trim())
        .ok_or_else(|| ApiError::bad_request("invalid_request", locale))?;
    if !upstream_names.contains_key(&fallback.to_ascii_lowercase()) {
        return Err(ApiError::bad_request("invalid_request", locale));
    }

    let mut rules = Vec::with_capacity(body.rules.len());
    for (index, rule) in body.rules.iter().enumerate() {
        let expression = rule.expression.trim();
        let upstream = chaos_core::config_render::normalized_dae_identifier(rule.upstream.trim())
            .ok_or_else(|| ApiError::bad_request("invalid_request", locale))?;
        if expression.is_empty()
            || expression.len() > 4096
            || expression
                .chars()
                .any(|character| matches!(character, '\r' | '\n'))
            || !upstream_names.contains_key(&upstream.to_ascii_lowercase())
        {
            return Err(ApiError::bad_request("invalid_request", locale));
        }
        rules.push((expression.to_string(), upstream, index as i64, rule.enabled));
    }
    let (saved_ups, saved_rules) =
        chaos_store::replace_dns(&state.pool, &ups, &rules, &fallback).await?;
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
        fallback,
    }))
}
