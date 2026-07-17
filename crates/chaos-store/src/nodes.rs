//! Node repository helpers.

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_rfc3339, Node};

pub async fn list_nodes(pool: &SqlitePool) -> Result<Vec<Node>, sqlx::Error> {
    sqlx::query_as::<_, Node>(
        r#"
        SELECT id, name, tag, link, protocol, address, subscription_id, created_at
        FROM nodes
        ORDER BY created_at DESC, id ASC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn get_node(pool: &SqlitePool, id: &str) -> Result<Option<Node>, sqlx::Error> {
    sqlx::query_as::<_, Node>(
        r#"
        SELECT id, name, tag, link, protocol, address, subscription_id, created_at
        FROM nodes
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Insert a node row. Caller supplies parsed fields; `id`/`created_at` are generated
/// unless `id` is provided (used when name depends on the final id).
pub async fn insert_node(
    pool: &SqlitePool,
    name: &str,
    tag: Option<&str>,
    link: &str,
    protocol: Option<&str>,
    address: Option<&str>,
    subscription_id: Option<&str>,
) -> Result<Node, sqlx::Error> {
    insert_node_with_id(
        pool,
        None,
        name,
        tag,
        link,
        protocol,
        address,
        subscription_id,
    )
    .await
}

/// Like [`insert_node`] but allows a pre-chosen id (e.g. for name = protocol + id prefix).
pub async fn insert_node_with_id(
    pool: &SqlitePool,
    id: Option<String>,
    name: &str,
    tag: Option<&str>,
    link: &str,
    protocol: Option<&str>,
    address: Option<&str>,
    subscription_id: Option<&str>,
) -> Result<Node, sqlx::Error> {
    let node = Node {
        id: id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: name.to_string(),
        tag: tag.map(|t| t.to_string()),
        link: link.to_string(),
        protocol: protocol.map(|p| p.to_string()),
        address: address.map(|a| a.to_string()),
        subscription_id: subscription_id.map(|s| s.to_string()),
        created_at: now_rfc3339(),
    };

    sqlx::query(
        r#"
        INSERT INTO nodes (id, name, tag, link, protocol, address, subscription_id, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(&node.id)
    .bind(&node.name)
    .bind(&node.tag)
    .bind(&node.link)
    .bind(&node.protocol)
    .bind(&node.address)
    .bind(&node.subscription_id)
    .bind(&node.created_at)
    .execute(pool)
    .await?;

    Ok(node)
}

/// Delete by id. Returns `true` if a row was deleted.
pub async fn delete_node(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM nodes WHERE id = ?1")
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
    async fn insert_list_delete_node() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        assert!(list_nodes(&pool).await.unwrap().is_empty());

        let created = insert_node(
            &pool,
            "n1",
            Some("tag"),
            "trojan://example@1.2.3.4:443",
            Some("trojan"),
            Some("1.2.3.4:443"),
            None,
        )
        .await
        .unwrap();

        let listed = list_nodes(&pool).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);
        assert_eq!(listed[0].name, "n1");
        assert_eq!(listed[0].protocol.as_deref(), Some("trojan"));

        assert!(get_node(&pool, &created.id).await.unwrap().is_some());
        assert!(delete_node(&pool, &created.id).await.unwrap());
        assert!(!delete_node(&pool, &created.id).await.unwrap());
        assert!(list_nodes(&pool).await.unwrap().is_empty());
    }
}
