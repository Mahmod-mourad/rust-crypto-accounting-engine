use anyhow::Result;
use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::config::DatabaseConfig;

pub async fn create_pool(cfg: &DatabaseConfig) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .connect(&cfg.url)
        .await?;
    Ok(pool)
}
