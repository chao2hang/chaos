//! User repository helpers.

use sqlx::SqlitePool;

use crate::models::User;

pub async fn count_users(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(n)
}

pub async fn create_user(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
) -> Result<User, sqlx::Error> {
    let user = User::new(username, password_hash);
    sqlx::query(
        r#"
        INSERT INTO users (id, username, password_hash, created_at)
        VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.password_hash)
    .bind(&user.created_at)
    .execute(pool)
    .await?;
    Ok(user)
}

pub async fn find_user_by_username(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, created_at
        FROM users
        WHERE username = ?1
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connect, migrate};

    #[tokio::test]
    async fn create_and_find_user() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        assert_eq!(count_users(&pool).await.unwrap(), 0);

        let created = create_user(&pool, "admin", "hash").await.unwrap();
        assert_eq!(created.username, "admin");
        assert_eq!(count_users(&pool).await.unwrap(), 1);

        let found = find_user_by_username(&pool, "admin").await.unwrap().unwrap();
        assert_eq!(found.id, created.id);
        assert_eq!(found.password_hash, "hash");

        assert!(find_user_by_username(&pool, "missing").await.unwrap().is_none());
    }
}
