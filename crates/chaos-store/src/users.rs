//! User repository helpers.
//!
//! **System rule:** when the database has no users, the account created via
//! install/setup is the **admin** (role = `admin`). Later accounts (if any)
//! default to `user`.

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
    create_user_with_role(pool, username, password_hash, User::ROLE_USER).await
}

/// Create the first install user (always admin).
pub async fn create_admin_user(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
) -> Result<User, sqlx::Error> {
    create_user_with_role(pool, username, password_hash, User::ROLE_ADMIN).await
}

/// Atomically create the install account only when the users table is empty.
/// SQLite serializes the conditional INSERT, so concurrent setup requests cannot
/// both observe an empty table and become administrators.
pub async fn create_first_admin_user(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
) -> Result<Option<User>, sqlx::Error> {
    let user = User::new_with_role(username, password_hash, User::ROLE_ADMIN);
    let result = sqlx::query(
        r#"
        INSERT INTO users (id, username, password_hash, created_at, role)
        SELECT ?1, ?2, ?3, ?4, ?5
        WHERE NOT EXISTS (SELECT 1 FROM users)
        "#,
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.password_hash)
    .bind(&user.created_at)
    .bind(&user.role)
    .execute(pool)
    .await?;
    Ok((result.rows_affected() == 1).then_some(user))
}

pub async fn find_user_by_id(pool: &SqlitePool, id: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, created_at, role
        FROM users
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn create_user_with_role(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
    role: &str,
) -> Result<User, sqlx::Error> {
    let user = User::new_with_role(username, password_hash, role);
    sqlx::query(
        r#"
        INSERT INTO users (id, username, password_hash, created_at, role)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.password_hash)
    .bind(&user.created_at)
    .bind(&user.role)
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
        SELECT id, username, password_hash, created_at, role
        FROM users
        WHERE username = ?1
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await
}

/// List all users.
pub async fn list_users(pool: &SqlitePool) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, created_at, role
        FROM users
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Delete a user by id. Returns true if a row was deleted.
pub async fn delete_user(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM users WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
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

        let created = create_admin_user(&pool, "admin", "hash").await.unwrap();
        assert_eq!(created.username, "admin");
        assert_eq!(created.role, User::ROLE_ADMIN);
        assert!(created.is_admin());
        assert_eq!(count_users(&pool).await.unwrap(), 1);

        let found = find_user_by_username(&pool, "admin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, created.id);
        assert_eq!(found.password_hash, "hash");
        assert_eq!(found.role, "admin");

        assert!(find_user_by_username(&pool, "missing")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn first_setup_user_is_admin() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let u = create_admin_user(&pool, "owner", "h").await.unwrap();
        assert_eq!(u.role, "admin");
        let second = create_user(&pool, "staff", "h2").await.unwrap();
        assert_eq!(second.role, "user");
        assert!(!second.is_admin());
    }
}
