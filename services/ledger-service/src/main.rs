// Scaffolded layers define types and traits that will be used once business
// logic is implemented. Allow dead_code until the codebase grows into them.
#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::signal;

mod api;
mod application;
mod config;
mod domain;
mod infrastructure;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env from the service directory (works regardless of CWD).
    // Falls back to the current directory so `cargo run` from inside the
    // service folder still works.
    let manifest_env = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    if manifest_env.exists() {
        dotenvy::from_path(&manifest_env).ok();
    } else {
        dotenvy::dotenv().ok();
    }

    // Initialise logs + distributed traces + Prometheus metrics
    let _telemetry = observability::init("ledger-service");

    // Load typed configuration from environment
    let config = config::AppConfig::from_env()?;

    tracing::info!(
        host = %config.server.host,
        port = config.server.port,
        "starting ledger-service"
    );

    // Establish database connection pool, run migrations, and build the trade
    // repository.  Non-fatal: the service falls back to in-memory-only mode
    // when PostgreSQL is unreachable.
    let trade_repo: Option<Arc<dyn domain::repository::TradeRepository>> =
        match infrastructure::db::create_pool(&config.database).await {
            Ok(pool) => {
                sqlx::migrate!("./migrations")
                    .run(&pool)
                    .await
                    .expect("failed to run database migrations");
                tracing::info!("database migrations applied");
                Some(Arc::new(infrastructure::repository::PgTradeRepository::new(pool)))
            }
            Err(err) => {
                tracing::warn!(%err, "database unavailable — running without persistence");
                None
            }
        };

    // Shared in-memory portfolio — RwLock allows concurrent reads (get_portfolio,
    // get_pnl) while write access (create_trade) remains exclusive.
    let portfolio = Arc::new(RwLock::new(domain::portfolio::Portfolio::new()));

    // Build Kafka producer. Non-fatal: runs without event publishing when
    // Kafka is unavailable (e.g. in local dev without a broker).
    let event_producer = match infrastructure::producer::KafkaTradeProducer::new(
        &config.kafka.brokers,
        &config.kafka.topic,
    ) {
        Ok(p) => {
            tracing::info!(
                brokers = %config.kafka.brokers,
                topic = %config.kafka.topic,
                "kafka producer ready"
            );
            Some(Arc::new(p))
        }
        Err(err) => {
            tracing::warn!(%err, "kafka unavailable — trade events will not be published");
            None
        }
    };

    // Build application state (wires services to the shared portfolio)
    let state = api::state::AppState::new(portfolio, trade_repo, event_producer);

    // Build the Axum router with state and middleware
    let router = api::router::build_router(state);

    // Bind the listener
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .expect("invalid server address");

    let listener = TcpListener::bind(addr).await?;

    tracing::info!(address = %addr, "server listening");

    // Serve with graceful shutdown
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("server shut down gracefully");

    Ok(())
}

/// Wait for SIGINT or SIGTERM and trigger graceful shutdown.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}
