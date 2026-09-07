//! chaos-store — SQLite pool, migrations, and row models.

pub mod api_keys;
pub mod config_plane;
pub mod config_profiles;
pub mod latency;
mod models;
pub mod nodes;
pub mod subscriptions;
pub mod users;

pub use api_keys::{
    create_api_key, find_active_api_key, list_api_keys, revoke_api_key, touch_api_key, ApiKey,
};
pub use config_plane::{
    add_group_member, clear_groups, delete_group, delete_meta, ensure_config_defaults, get_meta,
    insert_group, list_all_group_members, list_dns_rules, list_dns_upstreams, list_group_members,
    list_groups, list_routing_rules, publish_orchestration_v2, remove_group_member, replace_dns,
    replace_group_members, replace_routing_rules, restore_orchestration_publication,
    set_member_weight, set_meta, snapshot_orchestration_publication, update_group,
    OrchestrationPublicationSnapshot, PublishedGroup, PublishedOrchestration,
    PublishedOrchestrationPlan, PublishedRoutingRule, META_DNS_FALLBACK, META_NETWORK_DOCUMENT,
    META_ORCHESTRATION_DRAFT, META_ORCHESTRATION_FLOW, META_ORCHESTRATION_FLOW_LEGACY,
    META_ORCHESTRATION_NEEDS_REPUBLISH, META_ORCHESTRATION_PENDING, META_ORCHESTRATION_PLAN,
    META_ORCHESTRATION_V2_INITIALIZED, META_ROUTING_FALLBACK,
};
pub use config_profiles::{
    activate_profile, create_profile, delete_profile, get_active_profile, get_profile,
    list_profiles, update_profile, ConfigProfile,
};
pub use latency::{list_latency_results, list_latency_results_for_ids, upsert_latency_result};
pub use models::{
    now_rfc3339, parse_rfc3339, DnsRule, DnsUpstream, Group, GroupMember, LatencyResult, Node,
    RoutingRule, Subscription, User,
};
pub use nodes::{
    delete_node, get_node, insert_node, insert_node_with_id, list_nodes, update_node,
    update_node_country_code, NewNode, UpdateNode,
};
pub use subscriptions::{
    delete_subscription, get_subscription, insert_subscription, list_subscriptions,
    list_subscriptions_due_for_refresh, mark_subscription_refreshed, replace_subscription_nodes,
    set_subscription_refresh_schedule, update_subscription_meta, NewSubscriptionNode,
};
pub use users::{
    count_users, create_admin_user, create_first_admin_user, create_user, create_user_with_role,
    delete_user, find_user_by_id, find_user_by_username, list_users,
};

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

    if let Some(path) = sqlite_file_path(database_url) {
        secure_database_paths(path)?;
    }

    Ok(pool)
}

fn secure_database_paths(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for candidate in [
            path.to_path_buf(),
            Path::new(&format!("{}-wal", path.display())).to_path_buf(),
            Path::new(&format!("{}-shm", path.display())).to_path_buf(),
        ] {
            if candidate.exists() {
                std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o600))
                    .with_context(|| format!("chmod database file {}", candidate.display()))?;
            }
        }
    }
    Ok(())
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
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create database directory: {}", parent.display())
            })?;
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
            "api_keys",
        ] {
            let n: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap_or_else(|e| panic!("table {table} missing: {e}"));
            if matches!(
                table,
                "users" | "subscriptions" | "nodes" | "latency_results"
            ) {
                assert_eq!(n.0, 0, "expected empty {table}");
            }
        }
        let groups = list_groups(&pool).await.unwrap();
        assert_eq!(groups[0].name, "proxy");
    }
}
