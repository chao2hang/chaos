//! Subscription repository helpers.

use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::models::{now_rfc3339, Node, Subscription};

pub async fn list_subscriptions(pool: &SqlitePool) -> Result<Vec<Subscription>, sqlx::Error> {
    sqlx::query_as::<_, Subscription>(
        r#"
        SELECT id, tag, url, updated_at, status,
               refresh_interval_hours, last_refreshed_at, next_refresh_at
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
        SELECT id, tag, url, updated_at, status,
               refresh_interval_hours, last_refreshed_at, next_refresh_at
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
        refresh_interval_hours: 0,
        last_refreshed_at: None,
        next_refresh_at: None,
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
            id: n.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string()),
            name: n.name.clone(),
            tag: n.tag.clone(),
            link: n.link.clone(),
            protocol: n.protocol.clone(),
            address: n.address.clone(),
            subscription_id: Some(subscription_id.to_string()),
            created_at: created_at.clone(),
            country_code: None,
        };

        sqlx::query(
            r#"
            INSERT INTO nodes (id, name, tag, link, protocol, address, subscription_id, created_at, country_code)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
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
        .bind(&node.country_code)
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

/// Update the auto-refresh schedule for a subscription.
///
/// `interval_hours = 0` disables auto-refresh.
pub async fn set_subscription_refresh_schedule(
    pool: &SqlitePool,
    id: &str,
    interval_hours: i64,
) -> Result<Option<Subscription>, sqlx::Error> {
    let next_refresh_at = if interval_hours > 0 {
        let next = chrono::Utc::now() + chrono::Duration::hours(interval_hours);
        Some(next.to_rfc3339())
    } else {
        None
    };

    let result = sqlx::query(
        r#"
        UPDATE subscriptions
        SET refresh_interval_hours = ?1, next_refresh_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(interval_hours)
    .bind(&next_refresh_at)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }
    get_subscription(pool, id).await
}

/// Upper bound on how many intervals the scheduler will step through when
/// re-anchoring. Beyond this (a clock jump or a very long outage) the schedule
/// restarts from now instead of looping.
const MAX_SCHEDULE_STEPS: i64 = 100_000;

/// Work out the next refresh time for a subscription.
///
/// The next time is advanced by whole intervals from the *previously scheduled*
/// time, which keeps a stable cadence. Anchoring on the completion time instead
/// (`now + interval`) pushed each cycle later by however long the refresh took
/// plus up to one scheduler tick, so the schedule walked forward by minutes per
/// cycle and eventually fired hours later than configured.
pub fn next_refresh_at(
    previous: Option<&str>,
    interval_hours: i64,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    if interval_hours <= 0 {
        return None;
    }

    let interval = chrono::Duration::hours(interval_hours);
    let anchor = previous
        .and_then(|raw| chrono::DateTime::parse_from_rfc3339(raw).ok())
        .map(|parsed| parsed.with_timezone(&chrono::Utc))
        // No usable anchor (first refresh, or an unparsable stamp): start here.
        .unwrap_or(now);

    if anchor > now {
        // The schedule is still ahead of us; leave it where it is.
        return Some(anchor.to_rfc3339());
    }

    let interval_secs = interval.num_seconds();
    // Smallest k with anchor + k * interval > now.
    let steps = (now - anchor).num_seconds() / interval_secs + 1;
    if steps > MAX_SCHEDULE_STEPS {
        return Some((now + interval).to_rfc3339());
    }

    Some((anchor + interval * steps as i32).to_rfc3339())
}

/// Mark a subscription as refreshed and schedule the next refresh.
pub async fn mark_subscription_refreshed(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
    let now = now_rfc3339();
    let sub = get_subscription(pool, id).await?;
    let next_refresh_at = sub.as_ref().and_then(|s| {
        next_refresh_at(
            s.next_refresh_at.as_deref(),
            s.refresh_interval_hours,
            chrono::Utc::now(),
        )
    });

    sqlx::query(
        r#"
        UPDATE subscriptions
        SET last_refreshed_at = ?1, next_refresh_at = ?2, updated_at = ?1
        WHERE id = ?3
        "#,
    )
    .bind(&now)
    .bind(&next_refresh_at)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(())
}

/// List subscriptions that are due for auto-refresh.
pub async fn list_subscriptions_due_for_refresh(
    pool: &SqlitePool,
) -> Result<Vec<Subscription>, sqlx::Error> {
    let now = now_rfc3339();
    sqlx::query_as::<_, Subscription>(
        r#"
        SELECT id, tag, url, updated_at, status,
               refresh_interval_hours, last_refreshed_at, next_refresh_at
        FROM subscriptions
        WHERE refresh_interval_hours > 0
          AND next_refresh_at IS NOT NULL
          AND next_refresh_at <= ?1
        ORDER BY next_refresh_at ASC
        "#,
    )
    .bind(&now)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connect, list_nodes, migrate};

    fn at(raw: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(raw)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn next_refresh_keeps_a_stable_cadence() {
        // A 6h interval anchored at 08:17:20, observed five minutes late (the
        // scheduler tick granularity) must schedule 14:17:20 — not 14:22:20,
        // which is what anchoring on completion time produced, walking the
        // schedule forward minutes per cycle.
        let next = next_refresh_at(
            Some("2026-09-18T08:17:20+00:00"),
            6,
            at("2026-09-18T08:22:20+00:00"),
        )
        .unwrap();

        assert_eq!(next, at("2026-09-18T14:17:20+00:00").to_rfc3339());
    }

    #[test]
    fn next_refresh_steps_over_missed_intervals() {
        // A long outage must not queue up one refresh per missed interval.
        let next = next_refresh_at(
            Some("2026-09-18T08:17:20+00:00"),
            1,
            at("2026-09-18T12:30:00+00:00"),
        )
        .unwrap();

        assert_eq!(next, at("2026-09-18T13:17:20+00:00").to_rfc3339());
    }

    #[test]
    fn next_refresh_keeps_a_future_anchor_and_handles_missing_or_invalid_input() {
        // Already scheduled ahead: do not move it.
        let anchor = "2026-09-18T20:00:00+00:00";
        assert_eq!(
            next_refresh_at(Some(anchor), 6, at("2026-09-18T12:00:00+00:00")).unwrap(),
            at(anchor).to_rfc3339()
        );

        // No anchor, unparsable anchor and disabled interval.
        let now = at("2026-09-18T12:00:00+00:00");
        assert_eq!(
            next_refresh_at(None, 6, now).unwrap(),
            at("2026-09-18T18:00:00+00:00").to_rfc3339()
        );
        assert_eq!(
            next_refresh_at(Some("garbage"), 6, now).unwrap(),
            at("2026-09-18T18:00:00+00:00").to_rfc3339()
        );
        assert_eq!(next_refresh_at(Some(anchor), 0, now), None);
        assert_eq!(next_refresh_at(Some(anchor), -3, now), None);
    }

    #[test]
    fn next_refresh_falls_back_when_anchor_is_absurdly_old() {
        // A clock jump far into the past must not spin through millions of steps.
        let now = at("2026-09-18T10:00:00+00:00");
        let next = next_refresh_at(Some("1970-01-01T00:00:00+00:00"), 1, now).unwrap();

        assert_eq!(next, at("2026-09-18T11:00:00+00:00").to_rfc3339());
    }

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
