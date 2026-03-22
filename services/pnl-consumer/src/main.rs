use anyhow::Result;

mod application;
mod config;
mod domain;
mod infrastructure;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present (non-fatal if missing).
    let _ = dotenvy::dotenv();

    let _telemetry = observability::init("pnl-consumer");

    let cfg = config::AppConfig::from_env()?;

    tracing::info!(
        brokers  = %cfg.kafka.brokers,
        topic    = %cfg.kafka.topic,
        group_id = %cfg.kafka.group_id,
        "pnl-consumer starting"
    );

    // ── Database ──────────────────────────────────────────────────────────────
    let pool = infrastructure::db::create_pool(&cfg.database).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("database migrations applied");

    // ── Wire up and run ───────────────────────────────────────────────────────
    let repo = infrastructure::repository::PnlRepository::new(pool);
    let processor = application::processor::EventProcessor::new(repo);
    let consumer = infrastructure::consumer::KafkaConsumer::new(&cfg.kafka, processor)?;

    consumer.run().await
}
