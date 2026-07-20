//! Groups, routing, DNS, and config_meta (P3/P4 config plane).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_rfc3339, DnsRule, DnsUpstream, Group, GroupMember, RoutingRule};

pub const META_ROUTING_FALLBACK: &str = "routing.fallback";
pub const META_DNS_FALLBACK: &str = "dns.fallback";
/// Last successfully published V2 graph. Runtime compilation must only read this key.
pub const META_ORCHESTRATION_FLOW: &str = "orchestration.flow.v2";
/// Editable V2 graph. Saving a draft must never change the active runtime graph.
pub const META_ORCHESTRATION_DRAFT: &str = "orchestration.flow.v2.draft";
/// Expanded, immutable runtime resources produced by the last V2 publication.
pub const META_ORCHESTRATION_PLAN: &str = "orchestration.plan.v2";
/// Durable marker used to recover a publication interrupted between the DB
/// commit and the data-plane apply.
pub const META_ORCHESTRATION_PENDING: &str = "orchestration.pending.v2";
/// Set when a source resource changed after publication and the immutable
/// expanded plan must be published again before it can be applied.
pub const META_ORCHESTRATION_NEEDS_REPUBLISH: &str = "orchestration.needs_republish";
pub const META_ORCHESTRATION_FLOW_LEGACY: &str = "orchestration.flow.v1";
pub const META_ORCHESTRATION_V2_INITIALIZED: &str = "orchestration.v2.initialized";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedGroup {
    pub node_id: String,
    pub id: String,
    pub name: String,
    pub policy: String,
    pub members: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedRoutingRule {
    pub expression: String,
    pub outbound: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedOrchestrationPlan {
    pub document: String,
    pub groups: Vec<PublishedGroup>,
    pub routing: Vec<PublishedRoutingRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedOrchestration {
    pub group_ids: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationPublicationSnapshot {
    pub routing: Vec<RoutingRule>,
    pub routing_fallback: Option<String>,
    pub flow: Option<String>,
    pub draft: Option<String>,
    pub plan: Option<String>,
    pub needs_republish: Option<String>,
    pub legacy_flow: Option<String>,
    pub initialized: Option<String>,
}

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

pub async fn delete_meta(pool: &SqlitePool, key: &str) -> Result<()> {
    sqlx::query("DELETE FROM config_meta WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await
        .context("delete_meta")?;
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

/// Remove every configured proxy group and its memberships. Imported nodes and subscriptions
/// live in separate tables and are intentionally left untouched.
pub async fn clear_groups(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await.context("begin clear groups")?;
    sqlx::query("DELETE FROM group_members")
        .execute(&mut *tx)
        .await
        .context("clear group_members")?;
    sqlx::query("DELETE FROM groups")
        .execute(&mut *tx)
        .await
        .context("clear groups")?;
    tx.commit().await.context("commit clear groups")?;
    Ok(())
}

/// Publish a complete V2 orchestration config-plane replacement atomically.
/// Imported nodes and subscriptions are deliberately outside this transaction.
pub async fn publish_orchestration_v2(
    pool: &SqlitePool,
    plan: &PublishedOrchestrationPlan,
) -> Result<PublishedOrchestration> {
    let mut tx = pool.begin().await.context("begin orchestration publish")?;

    // `groups` is the reusable source-group catalog. Runtime groups belong to the
    // published plan and must not overwrite source groups that a graph references.
    let group_ids = plan
        .groups
        .iter()
        .map(|group| (group.node_id.clone(), group.id.clone()))
        .collect();

    sqlx::query("DELETE FROM routing_rules")
        .execute(&mut *tx)
        .await
        .context("clear orchestration routing rules")?;
    for (sort_order, rule) in plan.routing.iter().enumerate() {
        sqlx::query(
            "INSERT INTO routing_rules (id, expression, outbound, sort_order, enabled)
             VALUES (?, ?, ?, ?, 1)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&rule.expression)
        .bind(&rule.outbound)
        .bind(sort_order as i64)
        .execute(&mut *tx)
        .await
        .context("insert orchestration routing rule")?;
    }

    let plan_json = serde_json::to_string(plan).context("serialize orchestration plan")?;
    for (key, value) in [
        (META_ROUTING_FALLBACK, "direct"),
        (META_ORCHESTRATION_FLOW, plan.document.as_str()),
        (META_ORCHESTRATION_PLAN, plan_json.as_str()),
        (META_ORCHESTRATION_V2_INITIALIZED, "true"),
    ] {
        sqlx::query(
            "INSERT INTO config_meta (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await
        .context("save orchestration metadata")?;
    }
    sqlx::query("DELETE FROM config_meta WHERE key = ?")
        .bind(META_ORCHESTRATION_FLOW_LEGACY)
        .execute(&mut *tx)
        .await
        .context("remove legacy orchestration metadata")?;
    sqlx::query("DELETE FROM config_meta WHERE key = ?")
        .bind(META_ORCHESTRATION_DRAFT)
        .execute(&mut *tx)
        .await
        .context("remove published orchestration draft")?;
    sqlx::query("DELETE FROM config_meta WHERE key = ?")
        .bind(META_ORCHESTRATION_NEEDS_REPUBLISH)
        .execute(&mut *tx)
        .await
        .context("clear orchestration republish marker")?;

    tx.commit().await.context("commit orchestration publish")?;
    Ok(PublishedOrchestration { group_ids })
}

pub async fn snapshot_orchestration_publication(
    pool: &SqlitePool,
) -> Result<OrchestrationPublicationSnapshot> {
    Ok(OrchestrationPublicationSnapshot {
        routing: list_routing_rules(pool).await?,
        routing_fallback: get_meta(pool, META_ROUTING_FALLBACK).await?,
        flow: get_meta(pool, META_ORCHESTRATION_FLOW).await?,
        draft: get_meta(pool, META_ORCHESTRATION_DRAFT).await?,
        plan: get_meta(pool, META_ORCHESTRATION_PLAN).await?,
        needs_republish: get_meta(pool, META_ORCHESTRATION_NEEDS_REPUBLISH).await?,
        legacy_flow: get_meta(pool, META_ORCHESTRATION_FLOW_LEGACY).await?,
        initialized: get_meta(pool, META_ORCHESTRATION_V2_INITIALIZED).await?,
    })
}

pub async fn restore_orchestration_publication(
    pool: &SqlitePool,
    snapshot: &OrchestrationPublicationSnapshot,
    failed_draft: &str,
) -> Result<()> {
    let mut tx = pool.begin().await.context("begin orchestration restore")?;
    sqlx::query("DELETE FROM routing_rules")
        .execute(&mut *tx)
        .await
        .context("clear failed routing publication")?;
    for rule in &snapshot.routing {
        sqlx::query(
            "INSERT INTO routing_rules (id, expression, outbound, sort_order, enabled)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&rule.id)
        .bind(&rule.expression)
        .bind(&rule.outbound)
        .bind(rule.sort_order)
        .bind(rule.enabled)
        .execute(&mut *tx)
        .await
        .context("restore routing rule")?;
    }

    for (key, value) in [
        (META_ROUTING_FALLBACK, snapshot.routing_fallback.as_deref()),
        (META_ORCHESTRATION_FLOW, snapshot.flow.as_deref()),
        (META_ORCHESTRATION_PLAN, snapshot.plan.as_deref()),
        (
            META_ORCHESTRATION_NEEDS_REPUBLISH,
            snapshot.needs_republish.as_deref(),
        ),
        (
            META_ORCHESTRATION_FLOW_LEGACY,
            snapshot.legacy_flow.as_deref(),
        ),
        (
            META_ORCHESTRATION_V2_INITIALIZED,
            snapshot.initialized.as_deref(),
        ),
        // Keep the rejected document as an editable draft.
        (META_ORCHESTRATION_DRAFT, Some(failed_draft)),
    ] {
        if let Some(value) = value {
            sqlx::query(
                "INSERT INTO config_meta (key, value) VALUES (?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await
            .context("restore orchestration metadata")?;
        } else {
            sqlx::query("DELETE FROM config_meta WHERE key = ?")
                .bind(key)
                .execute(&mut *tx)
                .await
                .context("remove failed orchestration metadata")?;
        }
    }
    sqlx::query("DELETE FROM config_meta WHERE key = ?")
        .bind(META_ORCHESTRATION_PENDING)
        .execute(&mut *tx)
        .await
        .context("clear pending orchestration publication")?;
    tx.commit().await.context("commit orchestration restore")?;
    Ok(())
}

// --- group members (proxy nodes + weight) ---

pub async fn list_group_members(pool: &SqlitePool, group_id: &str) -> Result<Vec<GroupMember>> {
    sqlx::query_as::<_, GroupMember>(
        "SELECT group_id, node_id, weight, sort_order
         FROM group_members WHERE group_id = ?
         ORDER BY sort_order ASC, node_id ASC",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await
    .context("list_group_members")
}

pub async fn list_all_group_members(pool: &SqlitePool) -> Result<Vec<GroupMember>> {
    sqlx::query_as::<_, GroupMember>(
        "SELECT group_id, node_id, weight, sort_order
         FROM group_members
         ORDER BY group_id ASC, sort_order ASC, node_id ASC",
    )
    .fetch_all(pool)
    .await
    .context("list_all_group_members")
}

/// Replace membership of a group. `members` is (node_id, weight, sort_order).
pub async fn replace_group_members(
    pool: &SqlitePool,
    group_id: &str,
    members: &[(String, i64, i64)],
) -> Result<Vec<GroupMember>> {
    let mut tx = pool.begin().await.context("begin replace members")?;
    sqlx::query("DELETE FROM group_members WHERE group_id = ?")
        .bind(group_id)
        .execute(&mut *tx)
        .await
        .context("clear group_members")?;
    let mut out = Vec::with_capacity(members.len());
    for (node_id, weight, sort_order) in members {
        let w = (*weight).max(1);
        sqlx::query(
            "INSERT INTO group_members (group_id, node_id, weight, sort_order)
             VALUES (?, ?, ?, ?)",
        )
        .bind(group_id)
        .bind(node_id)
        .bind(w)
        .bind(sort_order)
        .execute(&mut *tx)
        .await
        .context("insert group_member")?;
        out.push(GroupMember {
            group_id: group_id.to_string(),
            node_id: node_id.clone(),
            weight: w,
            sort_order: *sort_order,
        });
    }
    tx.commit().await.context("commit replace members")?;
    Ok(out)
}

pub async fn add_group_member(
    pool: &SqlitePool,
    group_id: &str,
    node_id: &str,
    weight: i64,
) -> Result<GroupMember> {
    let w = weight.max(1);
    let (max_order,): (Option<i64>,) =
        sqlx::query_as("SELECT MAX(sort_order) FROM group_members WHERE group_id = ?")
            .bind(group_id)
            .fetch_one(pool)
            .await
            .context("max sort_order")?;
    let sort_order = max_order.unwrap_or(-1) + 1;
    sqlx::query(
        "INSERT INTO group_members (group_id, node_id, weight, sort_order)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(group_id, node_id) DO UPDATE SET weight = excluded.weight",
    )
    .bind(group_id)
    .bind(node_id)
    .bind(w)
    .bind(sort_order)
    .execute(pool)
    .await
    .context("add_group_member")?;
    Ok(GroupMember {
        group_id: group_id.to_string(),
        node_id: node_id.to_string(),
        weight: w,
        sort_order,
    })
}

pub async fn remove_group_member(pool: &SqlitePool, group_id: &str, node_id: &str) -> Result<bool> {
    let res = sqlx::query("DELETE FROM group_members WHERE group_id = ? AND node_id = ?")
        .bind(group_id)
        .bind(node_id)
        .execute(pool)
        .await
        .context("remove_group_member")?;
    Ok(res.rows_affected() > 0)
}

pub async fn set_member_weight(
    pool: &SqlitePool,
    group_id: &str,
    node_id: &str,
    weight: i64,
) -> Result<bool> {
    let res = sqlx::query("UPDATE group_members SET weight = ? WHERE group_id = ? AND node_id = ?")
        .bind(weight.max(1))
        .bind(group_id)
        .bind(node_id)
        .execute(pool)
        .await
        .context("set_member_weight")?;
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
    let v2_initialized = get_meta(pool, META_ORCHESTRATION_V2_INITIALIZED)
        .await?
        .as_deref()
        == Some("true");

    // An empty group/routing table is a valid V2 direct-only publication. Re-seeding
    // legacy defaults here would make the runtime diverge from the persisted graph.
    if v2_initialized {
        if get_meta(pool, META_ROUTING_FALLBACK).await?.is_none() {
            set_meta(pool, META_ROUTING_FALLBACK, "direct").await?;
        }
        ensure_dns_defaults(pool).await?;
        return Ok(());
    }

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

    ensure_dns_defaults(pool).await?;

    Ok(())
}

async fn ensure_dns_defaults(pool: &SqlitePool) -> Result<()> {
    let (ucount,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM dns_upstreams")
        .fetch_one(pool)
        .await?;
    if ucount == 0 {
        replace_dns(
            pool,
            &[
                ("alidns".into(), "udp://dns.alidns.com:53".into(), 0),
                ("googledns".into(), "tcp+udp://dns.google:53".into(), 1),
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
    use crate::{connect, insert_node, migrate};

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
            get_meta(&pool, META_ROUTING_FALLBACK)
                .await
                .unwrap()
                .as_deref(),
            Some("proxy")
        );

        let ups = list_dns_upstreams(&pool).await.unwrap();
        assert_eq!(ups.len(), 2);

        replace_routing_rules(
            &pool,
            &[("domain(example.com)".into(), "proxy".into(), 0, true)],
            "direct",
        )
        .await
        .unwrap();
        let rules2 = list_routing_rules(&pool).await.unwrap();
        assert_eq!(rules2.len(), 1);
        assert_eq!(rules2[0].expression, "domain(example.com)");
        assert_eq!(
            get_meta(&pool, META_ROUTING_FALLBACK)
                .await
                .unwrap()
                .as_deref(),
            Some("direct")
        );
    }

    #[tokio::test]
    async fn publish_orchestration_replaces_generated_config() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let node = insert_node(
            &pool,
            "Old",
            None,
            "trojan://old.example:443",
            Some("trojan"),
            Some("old.example"),
            None,
        )
        .await
        .unwrap();
        let old_group = insert_group(&pool, "Old Group", "round-robin", None, 0)
            .await
            .unwrap();
        add_group_member(&pool, &old_group.id, &node.id, 1)
            .await
            .unwrap();
        replace_routing_rules(
            &pool,
            &[("domain(old.example)".into(), old_group.id.clone(), 0, true)],
            &old_group.id,
        )
        .await
        .unwrap();
        set_meta(&pool, META_ORCHESTRATION_FLOW_LEGACY, "legacy")
            .await
            .unwrap();
        set_meta(&pool, META_ORCHESTRATION_NEEDS_REPUBLISH, "true")
            .await
            .unwrap();

        let plan = PublishedOrchestrationPlan {
            document: "groups:\n  - id: proxy\n".to_string(),
            groups: vec![PublishedGroup {
                node_id: "proxy".to_string(),
                id: "published-proxy".to_string(),
                name: "Proxy".to_string(),
                policy: "round-robin".to_string(),
                members: vec![(node.id.clone(), 5)],
            }],
            routing: vec![PublishedRoutingRule {
                expression: "domain(example.com)".to_string(),
                outbound: "proxy".to_string(),
            }],
        };

        let published = publish_orchestration_v2(&pool, &plan).await.unwrap();
        let group_id = &published.group_ids[0].1;
        assert_eq!(published.group_ids[0].0, "proxy");
        // Source groups are not replaced by the runtime snapshot. This keeps a
        // group referenced by a V2 document available for the next publication.
        assert_eq!(list_groups(&pool).await.unwrap().len(), 2);
        assert_eq!(
            list_group_members(&pool, &old_group.id).await.unwrap()[0].weight,
            1
        );
        assert!(list_group_members(&pool, group_id)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            list_routing_rules(&pool).await.unwrap()[0].outbound,
            "proxy"
        );
        assert_eq!(
            get_meta(&pool, META_ORCHESTRATION_FLOW)
                .await
                .unwrap()
                .as_deref(),
            Some("groups:\n  - id: proxy\n")
        );
        assert_eq!(
            get_meta(&pool, META_ORCHESTRATION_V2_INITIALIZED)
                .await
                .unwrap()
                .as_deref(),
            Some("true")
        );
        let stored_plan = get_meta(&pool, META_ORCHESTRATION_PLAN)
            .await
            .unwrap()
            .unwrap();
        let decoded: PublishedOrchestrationPlan = serde_json::from_str(&stored_plan).unwrap();
        assert_eq!(decoded.groups[0].members[0].1, 5);
        assert!(get_meta(&pool, META_ORCHESTRATION_DRAFT)
            .await
            .unwrap()
            .is_none());
        assert!(get_meta(&pool, META_ORCHESTRATION_FLOW_LEGACY)
            .await
            .unwrap()
            .is_none());
        assert!(get_meta(&pool, META_ORCHESTRATION_NEEDS_REPUBLISH)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn v2_direct_only_publication_is_not_reseeded_on_restart() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        clear_groups(&pool).await.unwrap();
        replace_routing_rules(&pool, &[], "direct").await.unwrap();

        let plan = PublishedOrchestrationPlan {
            document: r#"{"version":2,"nodes":[],"edges":[],"viewport":{"x":0,"y":0,"zoom":1}}"#
                .to_string(),
            groups: vec![],
            routing: vec![],
        };
        publish_orchestration_v2(&pool, &plan).await.unwrap();

        // `migrate` runs this on every process start.
        ensure_config_defaults(&pool).await.unwrap();
        assert!(list_groups(&pool).await.unwrap().is_empty());
        assert!(list_routing_rules(&pool).await.unwrap().is_empty());
        assert_eq!(
            get_meta(&pool, META_ROUTING_FALLBACK)
                .await
                .unwrap()
                .as_deref(),
            Some("direct")
        );
    }

    #[tokio::test]
    async fn failed_publication_can_restore_the_previous_snapshot() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        set_meta(&pool, META_ORCHESTRATION_FLOW, "old-flow")
            .await
            .unwrap();
        set_meta(&pool, META_ORCHESTRATION_PLAN, "old-plan")
            .await
            .unwrap();
        set_meta(&pool, META_ORCHESTRATION_NEEDS_REPUBLISH, "true")
            .await
            .unwrap();
        replace_routing_rules(
            &pool,
            &[("domain(old.example)".into(), "direct".into(), 0, true)],
            "direct",
        )
        .await
        .unwrap();
        let snapshot = snapshot_orchestration_publication(&pool).await.unwrap();
        set_meta(
            &pool,
            META_ORCHESTRATION_PENDING,
            &serde_json::to_string(&snapshot).unwrap(),
        )
        .await
        .unwrap();

        publish_orchestration_v2(
            &pool,
            &PublishedOrchestrationPlan {
                document: "new-flow".into(),
                groups: vec![],
                routing: vec![PublishedRoutingRule {
                    expression: "domain(new.example)".into(),
                    outbound: "direct".into(),
                }],
            },
        )
        .await
        .unwrap();
        restore_orchestration_publication(&pool, &snapshot, "failed-draft")
            .await
            .unwrap();

        assert_eq!(
            get_meta(&pool, META_ORCHESTRATION_FLOW)
                .await
                .unwrap()
                .as_deref(),
            Some("old-flow")
        );
        assert_eq!(
            get_meta(&pool, META_ORCHESTRATION_PLAN)
                .await
                .unwrap()
                .as_deref(),
            Some("old-plan")
        );
        assert_eq!(
            get_meta(&pool, META_ORCHESTRATION_DRAFT)
                .await
                .unwrap()
                .as_deref(),
            Some("failed-draft")
        );
        assert_eq!(
            get_meta(&pool, META_ORCHESTRATION_NEEDS_REPUBLISH)
                .await
                .unwrap()
                .as_deref(),
            Some("true")
        );
        assert_eq!(
            list_routing_rules(&pool).await.unwrap()[0].expression,
            "domain(old.example)"
        );
        assert!(get_meta(&pool, META_ORCHESTRATION_PENDING)
            .await
            .unwrap()
            .is_none());
    }
}
