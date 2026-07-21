//! Config profile repository helpers.

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::now_rfc3339;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ConfigProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Serialized OrchestrationDocument JSON.
    pub document: String,
    pub is_active: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list_profiles(pool: &SqlitePool) -> Result<Vec<ConfigProfile>, sqlx::Error> {
    sqlx::query_as::<_, ConfigProfile>(
        r#"
        SELECT id, name, description, document, is_active, created_at, updated_at
        FROM config_profiles
        ORDER BY is_active DESC, updated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn get_profile(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<ConfigProfile>, sqlx::Error> {
    sqlx::query_as::<_, ConfigProfile>(
        r#"
        SELECT id, name, description, document, is_active, created_at, updated_at
        FROM config_profiles
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn get_active_profile(pool: &SqlitePool) -> Result<Option<ConfigProfile>, sqlx::Error> {
    sqlx::query_as::<_, ConfigProfile>(
        r#"
        SELECT id, name, description, document, is_active, created_at, updated_at
        FROM config_profiles
        WHERE is_active = 1
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await
}

pub async fn create_profile(
    pool: &SqlitePool,
    name: &str,
    description: &str,
    document: &str,
) -> Result<ConfigProfile, sqlx::Error> {
    let profile = ConfigProfile {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        description: description.to_string(),
        document: document.to_string(),
        is_active: 0,
        created_at: now_rfc3339(),
        updated_at: now_rfc3339(),
    };

    sqlx::query(
        r#"
        INSERT INTO config_profiles (id, name, description, document, is_active, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(&profile.id)
    .bind(&profile.name)
    .bind(&profile.description)
    .bind(&profile.document)
    .bind(profile.is_active)
    .bind(&profile.created_at)
    .bind(&profile.updated_at)
    .execute(pool)
    .await?;

    Ok(profile)
}

pub async fn update_profile(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    description: &str,
    document: &str,
) -> Result<Option<ConfigProfile>, sqlx::Error> {
    let updated_at = now_rfc3339();
    let result = sqlx::query(
        r#"
        UPDATE config_profiles
        SET name = ?1, description = ?2, document = ?3, updated_at = ?4
        WHERE id = ?5
        "#,
    )
    .bind(name)
    .bind(description)
    .bind(document)
    .bind(&updated_at)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }
    get_profile(pool, id).await
}

pub async fn delete_profile(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM config_profiles WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Activate a profile (deactivates all others).
pub async fn activate_profile(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<ConfigProfile>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Deactivate all
    sqlx::query("UPDATE config_profiles SET is_active = 0")
        .execute(&mut *tx)
        .await?;

    // Activate the target
    let updated_at = now_rfc3339();
    let result = sqlx::query(
        r#"
        UPDATE config_profiles
        SET is_active = 1, updated_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(&updated_at)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }
    get_profile(pool, id).await
}
