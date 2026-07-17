//! Latency result repository helpers.

use sqlx::SqlitePool;

use crate::models::LatencyResult;

/// Upsert one latency row keyed by `node_id`.
pub async fn upsert_latency_result(
    pool: &SqlitePool,
    node_id: &str,
    latency_ms: Option<i64>,
    alive: bool,
    tested_at: &str,
    message: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO latency_results (node_id, latency_ms, alive, tested_at, message)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(node_id) DO UPDATE SET
          latency_ms = excluded.latency_ms,
          alive = excluded.alive,
          tested_at = excluded.tested_at,
          message = excluded.message
        "#,
    )
    .bind(node_id)
    .bind(latency_ms)
    .bind(if alive { 1_i64 } else { 0_i64 })
    .bind(tested_at)
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

/// List all stored latency results (latest per node — table is one row per node).
pub async fn list_latency_results(pool: &SqlitePool) -> Result<Vec<LatencyResult>, sqlx::Error> {
    sqlx::query_as::<_, LatencyResult>(
        r#"
        SELECT node_id, latency_ms, alive, tested_at, message
        FROM latency_results
        ORDER BY tested_at DESC, node_id ASC
        "#,
    )
    .fetch_all(pool)
    .await
}

/// List latency results for a subset of node ids.
pub async fn list_latency_results_for_ids(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<LatencyResult>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    // SQLite has no array bind; filter in application for MVP batch sizes.
    let all = list_latency_results(pool).await?;
    Ok(all
        .into_iter()
        .filter(|r| ids.iter().any(|id| id == &r.node_id))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connect, insert_node, migrate};

    #[tokio::test]
    async fn upsert_and_list_latency() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        let node = insert_node(
            &pool,
            "n1",
            None,
            "trojan://x@1.2.3.4:443",
            Some("trojan"),
            Some("1.2.3.4:443"),
            None,
        )
        .await
        .unwrap();

        upsert_latency_result(&pool, &node.id, Some(42), true, "2026-01-01T00:00:00Z", None)
            .await
            .unwrap();

        let listed = list_latency_results(&pool).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].node_id, node.id);
        assert_eq!(listed[0].latency_ms, Some(42));
        assert_eq!(listed[0].alive, 1);

        upsert_latency_result(
            &pool,
            &node.id,
            None,
            false,
            "2026-01-01T00:01:00Z",
            Some("timeout"),
        )
        .await
        .unwrap();

        let listed = list_latency_results(&pool).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].latency_ms, None);
        assert_eq!(listed[0].alive, 0);
        assert_eq!(listed[0].message.as_deref(), Some("timeout"));
    }
}
