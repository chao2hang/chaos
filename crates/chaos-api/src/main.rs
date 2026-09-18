//! chaos-api — REST JSON API on 0.0.0.0:2030.

#[cfg(not(unix))]
compile_error!(
    "chaos-api is Unix-only: it drives the dae data plane through POSIX process and \
     network primitives. Windows support was removed in 0.1.27."
);

mod auth;
mod error;
mod health;
mod http;
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
use routes::update::update_router;
use routes::users::users_router;
use state::AppState;

/// Handle `--version` / `--help` without starting the server. Without this,
/// running `chaos-api --version` boots the whole process and then fails with
/// `Address already in use` whenever a server is already running.
fn handle_cli_flags() -> Option<i32> {
    let arg = std::env::args().nth(1)?;
    match arg.as_str() {
        "-V" | "--version" => {
            println!("chaos {}", env!("CARGO_PKG_VERSION"));
            Some(0)
        }
        "-h" | "--help" => {
            println!(
                "chaos-api {version}

USAGE:
    chaos-api [OPTIONS]

OPTIONS:
    -h, --help       Print this help
    -V, --version    Print version

ENVIRONMENT:
    CHAOS_BIND               Listen address (default 0.0.0.0:2030)
    CHAOS_DATABASE_URL       SQLite URL (default sqlite:./data/chaos.db?mode=rwc)
    CHAOS_WEB_DIR            Directory of the packaged web UI
    CHAOS_DAE_LOG_LEVEL      dae log level (trace|debug|info|warn|error|fatal)
    CHAOS_DAE_LOG_MAX_BYTES  dae.log rotation threshold in bytes
    CHAOS_DAE_ALLOW_SUDO     Allow dae to manage sudo-backed operations
    RUST_LOG                 tracing filter (default info)",
                version = env!("CARGO_PKG_VERSION")
            );
            Some(0)
        }
        _ => None,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Some(code) = handle_cli_flags() {
        std::process::exit(code);
    }

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
    // dae FATALs at startup when routing rules reference geoip()/geosite() and
    // the datasets are absent; make sure they exist before autostarting dae.
    routes::runtime::ensure_geo_datasets().await;
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
                .merge(users_router())
                .merge(update_router()),
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
    // `ConnectInfo` gives handlers the peer socket address, which the login
    // rate limiter uses to key attempts by client IP instead of by username.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
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
            let dir = ServeDir::new(&web_dir);
            // ServeDir::oneshot is infallible (Infallible error type).
            let res = dir
                .oneshot(html_req)
                .await
                .unwrap_or_else(|never| match never {});
            if res.status() != StatusCode::NOT_FOUND {
                return res.into_response();
            }
        }
    }

    // Exact file / directory / assets
    let dir = ServeDir::new(&web_dir).append_index_html_on_directories(true);
    let res = dir
        .oneshot(req)
        .await
        .unwrap_or_else(|never| match never {});
    if res.status() != StatusCode::NOT_FOUND {
        return res.into_response();
    }

    // SPA fallback shell
    if index.is_file() {
        let file = ServeFile::new(index);
        let res = file
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap_or_else(|never| match never {});
        return res.into_response();
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

    for sub in due {
        // Subscription URLs embed their access token in the path, so log a
        // redacted form (host only) instead of the credential-bearing URL.
        tracing::info!(
            id = %sub.id,
            url = %chaos_core::subscription::redact_subscription_url_for_log(&sub.url),
            "refreshing subscription"
        );
        match fetch_and_replace_subscription(state, &sub).await {
            Ok(node_count) => {
                tracing::info!(id = %sub.id, node_count, "subscription refreshed");
            }
            Err(e) => {
                tracing::warn!(id = %sub.id, error = %e, "subscription refresh failed");
                let _ =
                    chaos_store::update_subscription_meta(&state.pool, &sub.id, "refresh_failed")
                        .await;
            }
        }
        // Mark as refreshed (or failed) and schedule next.
        let _ = chaos_store::mark_subscription_refreshed(&state.pool, &sub.id).await;
    }
    Ok(())
}

/// Refresh one subscription. Delegates to the same store+guard logic as the
/// manual refresh endpoint: unchanged links keep their node ids (so group and
/// plan references survive), and a refresh that would delete nodes still
/// referenced by the published plan, an orchestration document, or a source
/// group fails closed instead of silently dropping configured members.
async fn fetch_and_replace_subscription(
    state: &AppState,
    sub: &chaos_store::Subscription,
) -> anyhow::Result<usize> {
    let (_, nodes) = crate::routes::subscriptions::fetch_and_replace_nodes(
        state,
        &sub.id,
        sub.tag.as_deref(),
        &sub.url,
        chaos_i18n::Locale::En,
    )
    .await?;
    Ok(nodes.len())
}
