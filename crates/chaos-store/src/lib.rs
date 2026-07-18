//! chaos-store — SQLite pool, migrations, and row models.

mod models;
pub mod config_plane;
pub mod latency;
pub mod nodes;
pub mod subscriptions;
pub mod users;

pub use config_plane::{
    add_group_member, delete_group, ensure_config_defaults, get_meta, insert_group,
    list_all_group_members, list_dns_rules, list_dns_upstreams, list_group_members, list_groups,
    list_routing_rules, remove_group_member, replace_dns, replace_group_members,
    replace_routing_rules, set_member_weight, set_meta, update_group, META_DNS_FALLBACK,
    META_ROUTING_FALLBACK,
};
pub use latency::{list_latency_results, list_latency_results_for_ids, upsert_latency_result};
pub use models::{
    DnsRule, DnsUpstream, Group, GroupMember, LatencyResult, Node, RoutingRule, Subscription, User,
    now_rfc3339, parse_rfc3339,
};
pub use nodes::{delete_node, get_node, insert_node, insert_node_with_id, list_nodes};
pub use subscriptions::{
    delete_subscription, get_subscription, insert_subscription, list_subscriptions,
    replace_subscription_nodes, update_subscription_meta, NewSubscriptionNode,
};
pub use users::{count_users, create_admin_user, create_user, find_user_by_username};

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

/// Connect to SQLite using `database_url` (e.g. `sqlite:./data/chaos.db?mode=rwc`
/// or `sqlite::memory:`). Creates the parent directory for file-backed URLs.
pub async fn connect(database_url: &str) -> Result<SqlitePool> {
    ensure_parent_dir(database_url)?;

    let options = SqliteConnectOptions::from_str(database_url)
        .with_context(|| format!("invalid database URL: {database_url}"))?
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .with_context(|| format!("failed to connect to database: {database_url}"))?;

    Ok(pool)
}

/// Run embedded migrations from `./migrations` (relative to this crate).
pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("failed to run database migrations")?;
    ensure_config_defaults(pool)
        .await
        .context("failed to seed config defaults")?;
    Ok(())
}

/// For file URLs like `sqlite:./data/chaos.db` or `sqlite:./data/chaos.db?mode=rwc`,
/// ensure the parent directory exists. No-op for memory or pure filename paths.
fn ensure_parent_dir(database_url: &str) -> Result<()> {
    let Some(path) = sqlite_file_path(database_url) else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create database directory: {}", parent.display()))?;
        }
    }

    Ok(())
}

fn sqlite_file_path(database_url: &str) -> Option<&Path> {
    let rest = database_url.strip_prefix("sqlite:")?;
    if rest == ":memory:" || rest.starts_with(":memory:") {
        return None;
    }
    // Drop query string: `./data/chaos.db?mode=rwc`
    let path_part = rest.split('?').next().unwrap_or(rest);
    if path_part.is_empty() {
        return None;
    }
    Some(Path::new(path_part))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrate_creates_users_table() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n.0, 0);
    }

    #[tokio::test]
    async fn migrate_creates_all_tables() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        for table in [
            "users",
            "subscriptions",
            "nodes",
            "latency_results",
            "groups",
            "routing_rules",
            "dns_upstreams",
            "dns_rules",
            "config_meta",
        ] {
            let n: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap_or_else(|e| panic!("table {table} missing: {e}"));
            if matches!(table, "users" | "subscriptions" | "nodes" | "latency_results") {
                assert_eq!(n.0, 0, "expected empty {table}");
            }
        }
        let groups = list_groups(&pool).await.unwrap();
        assert_eq!(groups[0].name, "proxy");
    }
}
