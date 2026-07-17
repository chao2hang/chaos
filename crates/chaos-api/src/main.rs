//! chaos-api — REST JSON API on 127.0.0.1:2030.

mod auth;
mod error;
mod health;
mod routes;
mod state;

use std::net::SocketAddr;

use axum::Router;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use auth::{auth_router, load_or_create_jwt_secret};
use health::health_router;
use routes::latency::latency_router;
use routes::nodes::nodes_router;
use routes::runtime::runtime_router;
use routes::subscriptions::subscriptions_router;
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

    let app = Router::new()
        .nest("/api/v1/auth", auth_router())
        .nest(
            "/api/v1",
            health_router()
                .merge(nodes_router())
                .merge(subscriptions_router())
                .merge(latency_router())
                .merge(runtime_router()),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = std::env::var("CHAOS_BIND")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 2030)));

    tracing::info!("listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
