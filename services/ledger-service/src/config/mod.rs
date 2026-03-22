use anyhow::{Context, Result};

/// Top-level application configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub kafka: KafkaConfig,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone)]
pub struct KafkaConfig {
    /// Comma-separated list of broker addresses, e.g. `localhost:9092`.
    pub brokers: String,
    /// Topic to publish trade events to.
    pub topic: String,
}

impl AppConfig {
    /// Load configuration from environment variables.
    /// Expects a `.env` file to be loaded before calling this.
    pub fn from_env() -> Result<Self> {
        Ok(AppConfig {
            server: ServerConfig {
                host: std::env::var("SERVER_HOST")
                    .unwrap_or_else(|_| "0.0.0.0".to_string()),
                port: std::env::var("SERVER_PORT")
                    .unwrap_or_else(|_| "3000".to_string())
                    .parse::<u16>()
                    .context("SERVER_PORT must be a valid port number")?,
            },
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL")
                    .context("DATABASE_URL must be set")?,
                max_connections: std::env::var("DATABASE_MAX_CONNECTIONS")
                    .unwrap_or_else(|_| "10".to_string())
                    .parse::<u32>()
                    .context("DATABASE_MAX_CONNECTIONS must be a valid integer")?,
            },
            kafka: KafkaConfig {
                brokers: std::env::var("KAFKA_BROKERS")
                    .unwrap_or_else(|_| "localhost:9092".to_string()),
                topic: std::env::var("KAFKA_TOPIC")
                    .unwrap_or_else(|_| "trades".to_string()),
            },
        })
    }
}
