//! chaos-api — REST JSON API on 0.0.0.0:2030.

mod auth;
mod error;
mod health;
mod locale;
mod routes;
mod state;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use auth::{auth_router, load_or_create_jwt_secret};
use health::health_router;
use routes::backup::backup_router;
use routes::config::config_router;
use routes::dns::dns_router;
use routes::groups::groups_router;
use routes::latency::latency_router;
use routes::network::network_router;
use routes::nodes::nodes_router;
use routes::orchestration::orchestration_router;
use routes::profiles::profiles_router;
use routes::routing::routing_router;
use routes::runtime::runtime_router;
use routes::subscriptions::subscriptions_router;
use routes::users::users_router;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url = std::env::var("CHAOS_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:./data/chaos.db?mode=rwc".to_string());

    let pool = chaos_store::connect(&database_url).await?;
    chaos_store::migrate(&pool).await?;

    let jwt_secret = load_or_create_jwt_secret()?;
    let state = AppState::new(pool, jwt_secret);
    routes::orchestration::recover_pending_publication(&state).await?;
    routes::runtime::restore_persisted_runtime().await;

    let mut app = Router::new()
        .nest("/api/v1/auth", auth_router())
        .nest(
            "/api/v1",
            health_router()
                .merge(nodes_router())
                .merge(subscriptions_router())
                .merge(latency_router())
                .merge(runtime_router())
                .merge(groups_router())
                .merge(routing_router())
                .merge(orchestration_router())
                .merge(dns_router())
                .merge(network_router())
                // Admin ops: backup/restore + config export. Profiles available for multi-config API.
                .merge(profiles_router())
                .merge(backup_router())
                .merge(config_router())
                .merge(users_router()),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // Release installs set CHAOS_WEB_DIR to the packaged SvelteKit static build.
    if let Some(web_dir) = std::env::var_os("CHAOS_WEB_DIR").map(PathBuf::from) {
        if web_dir.is_dir() {
            tracing::info!(path = %web_dir.display(), "serving web UI");
            app = app.fallback(move |req: Request<Body>| {
                let web_dir = web_dir.clone();
                async move { serve_web_ui(web_dir, req).await }
            });
        } else {
            tracing::warn!(
                path = %web_dir.display(),
                "CHAOS_WEB_DIR is set but is not a directory; UI will not be served"
            );
        }
    }

    // Spawn background subscription auto-refresh task.
    spawn_subscription_refresh_task(state);

    let addr: SocketAddr = std::env::var("CHAOS_BIND")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 2030)));

    tracing::info!("listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Serve packaged SvelteKit static assets.
/// Maps extensionless routes like `/dashboard` → `dashboard.html`, then SPA fallback.
async fn serve_web_ui(web_dir: PathBuf, req: Request<Body>) -> Response {
    let path = req.uri().path().to_string();
    let index = web_dir.join("index.html");

    // Never SPA-fallback API paths (or they look like 200 HTML success).
    if path == "/api" || path.starts_with("/api/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Extensionless app routes: try sibling `.html` first (adapter-static prerender).
    if path != "/"
        && !path.ends_with('/')
        && !Path::new(&path)
            .extension()
            .is_some_and(|ext| !ext.is_empty())
    {
        let html_uri = format!("{path}.html");
        if let Ok(uri) = html_uri.parse::<Uri>() {
            let mut html_req = Request::builder()
                .method(req.method().clone())
                .uri(uri)
                .body(Body::empty())
                .unwrap_or_else(|_| Request::new(Body::empty()));
            *html_req.headers_mut() = req.headers().clone();
            let mut dir = ServeDir::new(&web_dir);
            if let Ok(res) = dir.oneshot(html_req).await {
                if res.status() != StatusCode::NOT_FOUND {
                    return res.into_response();
                }
            }
        }
    }

    // Exact file / directory / assets
    let mut dir = ServeDir::new(&web_dir).append_index_html_on_directories(true);
    if let Ok(res) = dir.oneshot(req).await {
        if res.status() != StatusCode::NOT_FOUND {
            return res.into_response();
        }
    }

    // SPA fallback shell
    if index.is_file() {
        let mut file = ServeFile::new(index);
        if let Ok(res) = file
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
        {
            return res.into_response();
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

/// Background task that checks for subscriptions due for auto-refresh every 5 minutes.
fn spawn_subscription_refresh_task(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        // Skip the first immediate tick.
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(e) = refresh_due_subscriptions(&state).await {
                tracing::warn!(error = %e, "subscription auto-refresh cycle failed");
            }
        }
    });
}

async fn refresh_due_subscriptions(state: &AppState) -> anyhow::Result<()> {
    let due = chaos_store::list_subscriptions_due_for_refresh(&state.pool).await?;
    if due.is_empty() {
        return Ok(());
    }
    tracing::info!(count = due.len(), "auto-refreshing subscriptions");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    for sub in due {
        tracing::info!(id = %sub.id, url = %sub.url, "refreshing subscription");
        match fetch_and_replace_subscription(state, &client, &sub).await {
            Ok(node_count) => {
                tracing::info!(id = %sub.id, node_count, "subscription refreshed");
            }
            Err(e) => {
                tracing::warn!(id = %sub.id, error = %e, "subscription refresh failed");
                let _ = chaos_store::update_subscription_meta(&state.pool, &sub.id, "refresh_failed").await;
            }
        }
        // Mark as refreshed (or failed) and schedule next.
        let _ = chaos_store::mark_subscription_refreshed(&state.pool, &sub.id).await;
    }
    Ok(())
}

async fn fetch_and_replace_subscription(
    state: &AppState,
    client: &reqwest::Client,
    sub: &chaos_store::Subscription,
) -> anyhow::Result<usize> {
    use chaos_core::subscription::{decode_subscription_body, parse_subscription_links};

    let response = client.get(&sub.url).send().await?.error_for_status()?;
    let bytes = response.bytes().await?;
    let body = decode_subscription_body(&bytes);
    let links = parse_subscription_links(&body);

    let nodes: Vec<chaos_store::NewSubscriptionNode> = links
        .iter()
        .map(|link| {
            let protocol = chaos_core::link::detect_protocol(link);
            let address = chaos_core::link::detect_address(link);
            let link_tag = chaos_core::link::detect_tag(link);
            let id = uuid::Uuid::new_v4().to_string();
            let name = chaos_core::link::node_name(
                link_tag.as_deref(),
                sub.tag.as_deref(),
                protocol.as_deref(),
                &id,
            );
            chaos_store::NewSubscriptionNode {
                id: Some(id),
                name,
                tag: link_tag,
                link: link.clone(),
                protocol,
                address,
            }
        })
        .collect();

    let created = chaos_store::replace_subscription_nodes(&state.pool, &sub.id, "ok", &nodes).await?;
    Ok(created.len())
}
