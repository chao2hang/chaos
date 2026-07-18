//! Row models for SQLite tables.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Subscription {
    pub id: String,
    pub tag: Option<String>,
    pub url: String,
    pub updated_at: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub tag: Option<String>,
    pub link: String,
    pub protocol: Option<String>,
    pub address: Option<String>,
    pub subscription_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LatencyResult {
    pub node_id: String,
    pub latency_ms: Option<i64>,
    pub alive: i64,
    pub tested_at: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub policy: String,
    pub filter_tag: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RoutingRule {
    pub id: String,
    pub expression: String,
    pub outbound: String,
    pub sort_order: i64,
    pub enabled: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DnsUpstream {
    pub id: String,
    pub name: String,
    pub address: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DnsRule {
    pub id: String,
    pub expression: String,
    pub upstream: String,
    pub sort_order: i64,
    pub enabled: i64,
}

impl User {
    pub fn new(username: impl Into<String>, password_hash: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            username: username.into(),
            password_hash: password_hash.into(),
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

/// Convenience helpers for timestamp formatting used by later store modules.
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}
