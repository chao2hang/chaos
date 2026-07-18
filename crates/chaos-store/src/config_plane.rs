//! Groups, routing, DNS, and config_meta (P3/P4 config plane).

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{DnsRule, DnsUpstream, Group, RoutingRule, now_rfc3339};

pub const META_ROUTING_FALLBACK: &str = "routing.fallback";
pub const META_DNS_FALLBACK: &str = "dns.fallback";

// --- meta ---

pub async fn get_meta(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM config_meta WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .context("get_meta")?;
    Ok(row.map(|r| r.0))
}

pub async fn set_meta(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO config_meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .context("set_meta")?;
    Ok(())
}

// --- groups ---

pub async fn list_groups(pool: &SqlitePool) -> Result<Vec<Group>> {
    sqlx::query_as::<_, Group>(
        "SELECT id, name, policy, filter_tag, sort_order, created_at
         FROM groups ORDER BY sort_order ASC, name ASC",
    )
    .fetch_all(pool)
    .await
    .context("list_groups")
}

pub async fn insert_group(
    pool: &SqlitePool,
    name: &str,
    policy: &str,
    filter_tag: Option<&str>,
    sort_order: i64,
) -> Result<Group> {
    let id = Uuid::new_v4().to_string();
    let created_at = now_rfc3339();
    sqlx::query(
        "INSERT INTO groups (id, name, policy, filter_tag, sort_order, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(name)
    .bind(policy)
    .bind(filter_tag)
    .bind(sort_order)
    .bind(&created_at)
    .execute(pool)
    .await
    .context("insert_group")?;
    Ok(Group {
        id,
        name: name.to_string(),
        policy: policy.to_string(),
        filter_tag: filter_tag.map(|s| s.to_string()),
        sort_order,
        created_at,
    })
}

pub async fn update_group(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    policy: &str,
    filter_tag: Option<&str>,
    sort_order: i64,
) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE groups SET name = ?, policy = ?, filter_tag = ?, sort_order = ? WHERE id = ?",
    )
    .bind(name)
    .bind(policy)
    .bind(filter_tag)
    .bind(sort_order)
    .bind(id)
    .execute(pool)
    .await
    .context("update_group")?;
    Ok(res.rows_affected() > 0)
}

pub async fn delete_group(pool: &SqlitePool, id: &str) -> Result<bool> {
    let res = sqlx::query("DELETE FROM groups WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("delete_group")?;
    Ok(res.rows_affected() > 0)
}

// --- routing ---

pub async fn list_routing_rules(pool: &SqlitePool) -> Result<Vec<RoutingRule>> {
    sqlx::query_as::<_, RoutingRule>(
        "SELECT id, expression, outbound, sort_order, enabled
         FROM routing_rules ORDER BY sort_order ASC, id ASC",
    )
    .fetch_all(pool)
    .await
    .context("list_routing_rules")
}

pub async fn replace_routing_rules(
    pool: &SqlitePool,
    rules: &[(String, String, i64, bool)],
    fallback: &str,
) -> Result<Vec<RoutingRule>> {
    let mut tx = pool.begin().await.context("begin routing replace")?;
    sqlx::query("DELETE FROM routing_rules")
        .execute(&mut *tx)
        .await
        .context("clear routing_rules")?;
    let mut out = Vec::with_capacity(rules.len());
    for (expression, outbound, sort_order, enabled) in rules {
        let id = Uuid::new_v4().to_string();
        let en = if *enabled { 1i64 } else { 0 };
        sqlx::query(
            "INSERT INTO routing_rules (id, expression, outbound, sort_order, enabled)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(expression)
        .bind(outbound)
        .bind(sort_order)
        .bind(en)
        .execute(&mut *tx)
        .await
        .context("insert routing rule")?;
        out.push(RoutingRule {
            id,
            expression: expression.clone(),
            outbound: outbound.clone(),
            sort_order: *sort_order,
            enabled: en,
        });
    }
    sqlx::query(
        "INSERT INTO config_meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(META_ROUTING_FALLBACK)
    .bind(fallback)
    .execute(&mut *tx)
    .await
    .context("set routing fallback")?;
    tx.commit().await.context("commit routing replace")?;
    Ok(out)
}

// --- dns ---

pub async fn list_dns_upstreams(pool: &SqlitePool) -> Result<Vec<DnsUpstream>> {
    sqlx::query_as::<_, DnsUpstream>(
        "SELECT id, name, address, sort_order FROM dns_upstreams ORDER BY sort_order ASC, name ASC",
    )
    .fetch_all(pool)
    .await
    .context("list_dns_upstreams")
}

pub async fn list_dns_rules(pool: &SqlitePool) -> Result<Vec<DnsRule>> {
    sqlx::query_as::<_, DnsRule>(
        "SELECT id, expression, upstream, sort_order, enabled
         FROM dns_rules ORDER BY sort_order ASC, id ASC",
    )
    .fetch_all(pool)
    .await
    .context("list_dns_rules")
}

pub async fn replace_dns(
    pool: &SqlitePool,
    upstreams: &[(String, String, i64)],
    rules: &[(String, String, i64, bool)],
    fallback: &str,
) -> Result<(Vec<DnsUpstream>, Vec<DnsRule>)> {
    let mut tx = pool.begin().await.context("begin dns replace")?;
    sqlx::query("DELETE FROM dns_rules")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM dns_upstreams")
        .execute(&mut *tx)
        .await?;

    let mut up_out = Vec::with_capacity(upstreams.len());
    for (name, address, sort_order) in upstreams {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO dns_upstreams (id, name, address, sort_order) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(address)
        .bind(sort_order)
        .execute(&mut *tx)
        .await
        .context("insert dns upstream")?;
        up_out.push(DnsUpstream {
            id,
            name: name.clone(),
            address: address.clone(),
            sort_order: *sort_order,
        });
    }

    let mut rule_out = Vec::with_capacity(rules.len());
    for (expression, upstream, sort_order, enabled) in rules {
        let id = Uuid::new_v4().to_string();
        let en = if *enabled { 1i64 } else { 0 };
        sqlx::query(
            "INSERT INTO dns_rules (id, expression, upstream, sort_order, enabled)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(expression)
        .bind(upstream)
        .bind(sort_order)
        .bind(en)
        .execute(&mut *tx)
        .await
        .context("insert dns rule")?;
        rule_out.push(DnsRule {
            id,
            expression: expression.clone(),
            upstream: upstream.clone(),
            sort_order: *sort_order,
            enabled: en,
        });
    }

    sqlx::query(
        "INSERT INTO config_meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(META_DNS_FALLBACK)
    .bind(fallback)
    .execute(&mut *tx)
    .await
    .context("set dns fallback")?;
    tx.commit().await.context("commit dns replace")?;
    Ok((up_out, rule_out))
}

/// Seed default groups/routing/dns when tables are empty (fresh install or after migration).
pub async fn ensure_config_defaults(pool: &SqlitePool) -> Result<()> {
    let (gcount,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM groups")
        .fetch_one(pool)
        .await?;
    if gcount == 0 {
        insert_group(pool, "proxy", "min_moving_avg", None, 0).await?;
    }

    let (rcount,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM routing_rules")
        .fetch_one(pool)
        .await?;
    if rcount == 0 {
        replace_routing_rules(
            pool,
            &[(
                "pname(NetworkManager, systemd-resolved)".into(),
                "must_direct".into(),
                0,
                true,
            )],
            "proxy",
        )
        .await?;
    } else if get_meta(pool, META_ROUTING_FALLBACK).await?.is_none() {
        set_meta(pool, META_ROUTING_FALLBACK, "proxy").await?;
    }

    let (ucount,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM dns_upstreams")
        .fetch_one(pool)
        .await?;
    if ucount == 0 {
        replace_dns(
            pool,
            &[
                (
                    "alidns".into(),
                    "udp://dns.alidns.com:53".into(),
                    0,
                ),
                (
                    "googledns".into(),
                    "tcp+udp://dns.google:53".into(),
                    1,
                ),
            ],
            &[],
            "alidns",
        )
        .await?;
    } else if get_meta(pool, META_DNS_FALLBACK).await?.is_none() {
        set_meta(pool, META_DNS_FALLBACK, "alidns").await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connect, migrate};

    #[tokio::test]
    async fn seed_and_replace_roundtrip() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        ensure_config_defaults(&pool).await.unwrap();

        let groups = list_groups(&pool).await.unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "proxy");

        let rules = list_routing_rules(&pool).await.unwrap();
        assert!(!rules.is_empty());
        assert_eq!(
            get_meta(&pool, META_ROUTING_FALLBACK).await.unwrap().as_deref(),
            Some("proxy")
        );

        let ups = list_dns_upstreams(&pool).await.unwrap();
        assert_eq!(ups.len(), 2);

        replace_routing_rules(
            &pool,
            &[(
                "domain(example.com)".into(),
                "proxy".into(),
                0,
                true,
            )],
            "direct",
        )
        .await
        .unwrap();
        let rules2 = list_routing_rules(&pool).await.unwrap();
        assert_eq!(rules2.len(), 1);
        assert_eq!(rules2[0].expression, "domain(example.com)");
        assert_eq!(
            get_meta(&pool, META_ROUTING_FALLBACK).await.unwrap().as_deref(),
            Some("direct")
        );
    }
}
