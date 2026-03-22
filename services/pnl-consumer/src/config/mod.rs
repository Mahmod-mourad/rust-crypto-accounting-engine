use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub kafka: KafkaConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone)]
pub struct KafkaConfig {
    /// Comma-separated list of broker addresses, e.g. "localhost:9092".
    pub brokers: String,
    /// Topic to subscribe to (must match the ledger-service producer topic).
    pub topic: String,
    /// Consumer group ID — allows horizontal scaling with partition assignment.
    pub group_id: String,
    /// Where to start consuming when no committed offset exists for this group.
    pub offset_reset: String,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            kafka: KafkaConfig {
                brokers: std::env::var("KAFKA_BROKERS")
                    .unwrap_or_else(|_| "localhost:9092".into()),
                topic: std::env::var("KAFKA_TOPIC")
                    .unwrap_or_else(|_| "trades".into()),
                group_id: std::env::var("KAFKA_GROUP_ID")
                    .unwrap_or_else(|_| "pnl-consumer".into()),
                offset_reset: std::env::var("KAFKA_OFFSET_RESET")
                    .unwrap_or_else(|_| "earliest".into()),
            },
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL")
                    .context("DATABASE_URL is required")?,
                max_connections: std::env::var("DATABASE_MAX_CONNECTIONS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(10),
            },
        })
    }
}
