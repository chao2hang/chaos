//! Subscription repository helpers.

use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::models::{now_rfc3339, Node, Subscription};

pub async fn list_subscriptions(pool: &SqlitePool) -> Result<Vec<Subscription>, sqlx::Error> {
    sqlx::query_as::<_, Subscription>(
        r#"
        SELECT id, tag, url, updated_at, status
        FROM subscriptions
        ORDER BY updated_at DESC, id ASC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn get_subscription(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<Subscription>, sqlx::Error> {
    sqlx::query_as::<_, Subscription>(
        r#"
        SELECT id, tag, url, updated_at, status
        FROM subscriptions
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn insert_subscription(
    pool: &SqlitePool,
    tag: Option<&str>,
    url: &str,
    status: &str,
) -> Result<Subscription, sqlx::Error> {
    let sub = Subscription {
        id: Uuid::new_v4().to_string(),
        tag: tag.map(|t| t.to_string()),
        url: url.to_string(),
        updated_at: now_rfc3339(),
        status: status.to_string(),
    };

    sqlx::query(
        r#"
        INSERT INTO subscriptions (id, tag, url, updated_at, status)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&sub.id)
    .bind(&sub.tag)
    .bind(&sub.url)
    .bind(&sub.updated_at)
    .bind(&sub.status)
    .execute(pool)
    .await?;

    Ok(sub)
}

pub async fn update_subscription_meta(
    pool: &SqlitePool,
    id: &str,
    status: &str,
) -> Result<Option<Subscription>, sqlx::Error> {
    let updated_at = now_rfc3339();
    let result = sqlx::query(
        r#"
        UPDATE subscriptions
        SET status = ?1, updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(status)
    .bind(&updated_at)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }
    get_subscription(pool, id).await
}

/// Fields required to insert a node owned by a subscription.
#[derive(Debug, Clone)]
pub struct NewSubscriptionNode {
    /// Optional pre-chosen id (so callers can embed the id prefix in `name`).
    pub id: Option<String>,
    pub name: String,
    pub tag: Option<String>,
    pub link: String,
    pub protocol: Option<String>,
    pub address: Option<String>,
}

/// Delete existing nodes for `subscription_id` and insert `nodes` in one transaction.
/// Also bumps subscription `updated_at` and sets `status`.
pub async fn replace_subscription_nodes(
    pool: &SqlitePool,
    subscription_id: &str,
    status: &str,
    nodes: &[NewSubscriptionNode],
) -> Result<Vec<Node>, sqlx::Error> {
    let mut tx: Transaction<'_, Sqlite> = pool.begin().await?;

    sqlx::query("DELETE FROM nodes WHERE subscription_id = ?1")
        .bind(subscription_id)
        .execute(&mut *tx)
        .await?;

    let mut created = Vec::with_capacity(nodes.len());
    let created_at = now_rfc3339();

    for n in nodes {
        let node = Node {
            id: n
                .id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            name: n.name.clone(),
            tag: n.tag.clone(),
            link: n.link.clone(),
            protocol: n.protocol.clone(),
            address: n.address.clone(),
            subscription_id: Some(subscription_id.to_string()),
            created_at: created_at.clone(),
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
        .execute(&mut *tx)
        .await?;

        created.push(node);
    }

    let updated_at = now_rfc3339();
    sqlx::query(
        r#"
        UPDATE subscriptions
        SET status = ?1, updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(status)
    .bind(&updated_at)
    .bind(subscription_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(created)
}

/// Delete subscription by id (nodes cascade via FK). Returns true if a row was deleted.
pub async fn delete_subscription(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM subscriptions WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connect, migrate, list_nodes};

    #[tokio::test]
    async fn insert_replace_delete_subscription() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        let sub = insert_subscription(&pool, Some("sub1"), "https://example.com/sub", "ok")
            .await
            .unwrap();

        let listed = list_subscriptions(&pool).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, sub.id);

        let nodes = replace_subscription_nodes(
            &pool,
            &sub.id,
            "ok",
            &[NewSubscriptionNode {
                id: None,
                name: "n1".into(),
                tag: Some("sub1".into()),
                link: "trojan://u@1.2.3.4:443".into(),
                protocol: Some("trojan".into()),
                address: Some("1.2.3.4:443".into()),
            }],
        )
        .await
        .unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(list_nodes(&pool).await.unwrap().len(), 1);

        // Replace clears previous nodes.
        let nodes2 = replace_subscription_nodes(
            &pool,
            &sub.id,
            "ok",
            &[
                NewSubscriptionNode {
                    id: None,
                    name: "a".into(),
                    tag: None,
                    link: "vmess://x".into(),
                    protocol: Some("vmess".into()),
                    address: None,
                },
                NewSubscriptionNode {
                    id: None,
                    name: "b".into(),
                    tag: None,
                    link: "ss://y".into(),
                    protocol: Some("shadowsocks".into()),
                    address: None,
                },
            ],
        )
        .await
        .unwrap();
        assert_eq!(nodes2.len(), 2);
        assert_eq!(list_nodes(&pool).await.unwrap().len(), 2);

        assert!(delete_subscription(&pool, &sub.id).await.unwrap());
        assert!(list_subscriptions(&pool).await.unwrap().is_empty());
        assert!(
            list_nodes(&pool).await.unwrap().is_empty(),
            "nodes should cascade on subscription delete"
        );
    }
}
