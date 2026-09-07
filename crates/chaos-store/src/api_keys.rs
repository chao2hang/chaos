//! Persistent API-key metadata. The secret itself is never stored.

use crate::models::now_rfc3339;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApiKey {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    pub key_prefix: String,
    pub key_hash: String,
    pub scopes: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

pub async fn create_api_key(
    pool: &SqlitePool,
    owner_user_id: &str,
    name: &str,
    key_prefix: &str,
    key_hash: &str,
    scopes: &str,
) -> Result<ApiKey, sqlx::Error> {
    let key = ApiKey {
        id: Uuid::new_v4().to_string(),
        owner_user_id: owner_user_id.to_string(),
        name: name.to_string(),
        key_prefix: key_prefix.to_string(),
        key_hash: key_hash.to_string(),
        scopes: scopes.to_string(),
        created_at: now_rfc3339(),
        last_used_at: None,
        revoked_at: None,
    };
    sqlx::query(
        "INSERT INTO api_keys (id, owner_user_id, name, key_prefix, key_hash, scopes, created_at)\n         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&key.id)
    .bind(&key.owner_user_id)
    .bind(&key.name)
    .bind(&key.key_prefix)
    .bind(&key.key_hash)
    .bind(&key.scopes)
    .bind(&key.created_at)
    .execute(pool)
    .await?;
    Ok(key)
}

pub async fn find_active_api_key(
    pool: &SqlitePool,
    key_hash: &str,
) -> Result<Option<ApiKey>, sqlx::Error> {
    sqlx::query_as::<_, ApiKey>(
        "SELECT id, owner_user_id, name, key_prefix, key_hash, scopes, created_at, last_used_at, revoked_at\n         FROM api_keys WHERE key_hash = ? AND revoked_at IS NULL",
    )
    .bind(key_hash)
    .fetch_optional(pool)
    .await
}

pub async fn touch_api_key(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ?")
        .bind(now_rfc3339())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_api_keys(
    pool: &SqlitePool,
    owner_user_id: &str,
) -> Result<Vec<ApiKey>, sqlx::Error> {
    sqlx::query_as::<_, ApiKey>(
        "SELECT id, owner_user_id, name, key_prefix, key_hash, scopes, created_at, last_used_at, revoked_at\n         FROM api_keys WHERE owner_user_id = ? ORDER BY created_at DESC",
    )
    .bind(owner_user_id)
    .fetch_all(pool)
    .await
}

pub async fn revoke_api_key(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("UPDATE api_keys SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
            .bind(now_rfc3339())
            .bind(id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}
