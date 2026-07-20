//! Routing document GET/PUT (auth required).

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::ApiError;
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
    // V2 owns routing mutations. This endpoint is a generated read model.
    Router::new().route("/routing", get(get_routing))
}

async fn get_routing(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<RoutingDocument>, ApiError> {
    if let Some(raw) =
        chaos_store::get_meta(&state.pool, chaos_store::META_ORCHESTRATION_PLAN).await?
    {
        let plan: chaos_store::PublishedOrchestrationPlan = serde_json::from_str(&raw)
            .map_err(|error| ApiError::internal_logged(chaos_i18n::Locale::En, error))?;
        let document: chaos_core::orchestration::OrchestrationDocument =
            serde_json::from_str(&plan.document)
                .map_err(|error| ApiError::internal_logged(chaos_i18n::Locale::En, error))?;
        let compiled = document.compile().map_err(|report| {
            tracing::error!(issues = ?report.issues, "published routing graph is invalid");
            ApiError::internal(chaos_i18n::Locale::En)
        })?;
        return Ok(Json(RoutingDocument {
            rules: compiled
                .conditions
                .into_iter()
                .map(|route| RoutingRuleDto {
                    expression: route.condition,
                    outbound: route.outbound,
                    enabled: true,
                })
                .collect(),
            fallback: compiled.fallback,
        }));
    }
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
