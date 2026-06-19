mod codec;
mod config;
mod game;
mod session;
mod socket;

use std::time::Duration;

use axum::{routing::get, Router};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

use game::GameRegistry;
use socket::registry::ConnectionRegistry;

#[derive(Clone)]
pub struct AppState {
    pub redis_client: redis::Client,
    pub connections: ConnectionRegistry,
    pub registry: GameRegistry,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = config::Config::from_env();

    let redis_client = redis::Client::open(cfg.redis_url.as_str())?;

    // Fail fast if Redis is unreachable.
    let mut test_conn = redis_client.get_multiplexed_async_connection().await?;
    redis::cmd("PING")
        .query_async::<String>(&mut test_conn)
        .await
        .map_err(|e| anyhow::anyhow!("Redis ping failed: {}", e))?;
    info!("Redis connection OK");

    let state = AppState {
        redis_client,
        connections: ConnectionRegistry::new(),
        registry: GameRegistry::new(),
    };

    let app = Router::new()
        .route("/ws", get(socket::handler::ws_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(state.clone());

    let listener = TcpListener::bind(&cfg.bind_addr).await?;
    info!(addr = %cfg.bind_addr, "game-gateway listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("listener closed, draining active game actors");
    state.registry.drain(Duration::from_secs(30)).await;
    info!("shutdown complete");

    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let sigterm = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c  => info!("received Ctrl+C"),
        _ = sigterm => info!("received SIGTERM"),
    }
}
