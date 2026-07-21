//! Traffic statistics and reporting.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    /// Time range: "1h", "24h", "7d", "30d"
    #[serde(default = "default_range")]
    pub range: String,
    /// Optional node ID filter
    pub node_id: Option<String>,
}

fn default_range() -> String {
    "24h".to_string()
}

#[derive(Debug, Serialize)]
pub struct TrafficStatsResponse {
    pub range: String,
    pub total_bytes_up: i64,
    pub total_bytes_down: i64,
    pub total_connections: i64,
    pub by_node: Vec<NodeTrafficStats>,
}

#[derive(Debug, Serialize)]
pub struct NodeTrafficStats {
    pub node_id: String,
    pub bytes_up: i64,
    pub bytes_down: i64,
    pub connections: i64,
}

/// Get traffic statistics for a time range.
async fn get_traffic_stats(
    _user: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<StatsQuery>,
) -> Result<Json<TrafficStatsResponse>, ApiError> {
    // Calculate time cutoff based on range
    let cutoff = match params.range.as_str() {
        "1h" => chrono::Utc::now() - chrono::Duration::hours(1),
        "24h" => chrono::Utc::now() - chrono::Duration::hours(24),
        "7d" => chrono::Utc::now() - chrono::Duration::days(7),
        "30d" => chrono::Utc::now() - chrono::Duration::days(30),
        _ => chrono::Utc::now() - chrono::Duration::hours(24),
    };
    let cutoff_str = cutoff.to_rfc3339();

    // Query aggregated stats
    let query = if let Some(ref node_id) = params.node_id {
        sqlx::query_as::<_, (String, i64, i64, i64)>(
            r#"
            SELECT node_id, 
                   COALESCE(SUM(bytes_up), 0), 
                   COALESCE(SUM(bytes_down), 0),
                   COALESCE(SUM(connections), 0)
            FROM traffic_stats
            WHERE timestamp >= ?1 AND node_id = ?2
            GROUP BY node_id
            "#,
        )
        .bind(&cutoff_str)
        .bind(node_id)
    } else {
        sqlx::query_as::<_, (String, i64, i64, i64)>(
            r#"
            SELECT node_id, 
                   COALESCE(SUM(bytes_up), 0), 
                   COALESCE(SUM(bytes_down), 0),
                   COALESCE(SUM(connections), 0)
            FROM traffic_stats
            WHERE timestamp >= ?1
            GROUP BY node_id
            "#,
        )
        .bind(&cutoff_str)
    };

    let rows: Vec<(String, i64, i64, i64)> = query.fetch_all(&state.pool).await?;

    let mut total_up = 0i64;
    let mut total_down = 0i64;
    let mut total_conns = 0i64;
    let mut by_node = Vec::new();

    for (node_id, up, down, conns) in rows {
        total_up += up;
        total_down += down;
        total_conns += conns;
        by_node.push(NodeTrafficStats {
            node_id,
            bytes_up: up,
            bytes_down: down,
            connections: conns,
        });
    }

    // Sort by total traffic descending
    by_node.sort_by(|a, b| {
        (b.bytes_up + b.bytes_down).cmp(&(a.bytes_up + a.bytes_down))
    });

    Ok(Json(TrafficStatsResponse {
        range: params.range,
        total_bytes_up: total_up,
        total_bytes_down: total_down,
        total_connections: total_conns,
        by_node,
    }))
}

pub fn stats_router() -> Router<AppState> {
    Router::new().route("/stats/traffic", get(get_traffic_stats))
}
