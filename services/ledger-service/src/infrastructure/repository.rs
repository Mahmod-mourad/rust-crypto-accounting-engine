// Concrete repository implementations backed by PostgreSQL via SQLx.
//
// Uses runtime query functions (`sqlx::query` / `sqlx::query_as`) rather than
// the compile-time macros so the service compiles without a live database or
// an offline `.sqlx` cache.  Parameterised bindings ($1 … $N) prevent SQL
// injection at the driver level.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::{
    errors::TradeError,
    repository::TradeRepository,
    trade::{Trade, TradeSide},
};

// ─── Repository struct ────────────────────────────────────────────────────────

pub struct PgTradeRepository {
    pool: PgPool,
}

impl PgTradeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// ─── Internal row type ────────────────────────────────────────────────────────

/// Intermediate type that SQLx can deserialise directly from a `trades` row.
/// Converted into the domain `Trade` via `TryFrom`.
#[derive(sqlx::FromRow)]
struct TradeRow {
    id: Uuid,
    asset: String,
    quantity: Decimal,
    price: Decimal,
    side: String,
    timestamp: DateTime<Utc>,
}

impl TryFrom<TradeRow> for Trade {
    type Error = TradeError;

    fn try_from(row: TradeRow) -> Result<Self, Self::Error> {
        let side = match row.side.as_str() {
            "buy" => TradeSide::Buy,
            "sell" => TradeSide::Sell,
            other => {
                return Err(TradeError::InvalidTrade(format!(
                    "unknown trade side in database: {other}"
                )))
            }
        };

        Ok(Trade {
            id: row.id,
            asset: row.asset,
            quantity: row.quantity,
            price: row.price,
            side,
            timestamp: row.timestamp,
        })
    }
}

// ─── TradeRepository implementation ──────────────────────────────────────────

#[async_trait]
impl TradeRepository for PgTradeRepository {
    /// Persist a trade.  `ON CONFLICT (id) DO NOTHING` makes the call
    /// idempotent, so retrying a failed request is safe.
    async fn save_trade(&self, trade: &Trade) -> Result<(), TradeError> {
        let side = match trade.side {
            TradeSide::Buy => "buy",
            TradeSide::Sell => "sell",
        };

        sqlx::query(
            r#"
            INSERT INTO trades (id, asset, quantity, price, side, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(trade.id)
        .bind(&trade.asset)
        .bind(trade.quantity)
        .bind(trade.price)
        .bind(side)
        .bind(trade.timestamp)
        .execute(&self.pool)
        .await
        .map_err(|e| TradeError::Persistence(e.to_string()))?;

        Ok(())
    }

    /// Load all trades, newest first.
    async fn get_trades(&self) -> Result<Vec<Trade>, TradeError> {
        let rows = sqlx::query_as::<_, TradeRow>(
            "SELECT id, asset, quantity, price, side, timestamp \
             FROM trades \
             ORDER BY timestamp DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| TradeError::Persistence(e.to_string()))?;

        rows.into_iter().map(Trade::try_from).collect()
    }
}
