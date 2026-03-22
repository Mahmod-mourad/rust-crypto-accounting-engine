use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::config::DatabaseConfig;

/// Create and validate a PostgreSQL connection pool.
pub async fn create_pool(config: &DatabaseConfig) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .connect(&config.url)
        .await?;

    tracing::info!(
        max_connections = config.max_connections,
        "database connection pool established"
    );

    Ok(pool)
}
