use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::{portfolio::AssetPortfolio, trade_event::TradeEvent};

// ─── Row types (sqlx::FromRow) ────────────────────────────────────────────────

/// Deserialized row from `portfolio_state`.
#[derive(sqlx::FromRow)]
struct PortfolioRow {
    lots: serde_json::Value,
    total_quantity: Decimal,
    total_realized_pnl: Decimal,
}

// ─── Repository ───────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PnlRepository {
    pool: PgPool,
}

impl PnlRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>> {
        Ok(self.pool.begin().await?)
    }

    // ── Idempotency ───────────────────────────────────────────────────────────

    /// Try to claim `event_id` for processing.
    ///
    /// Returns `true` if the row was inserted (first time seen → safe to
    /// process) or `false` if it already existed (duplicate → skip).
    pub async fn claim_event(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event: &TradeEvent,
        kafka_partition: i32,
        kafka_offset: i64,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO processed_events
                (event_id, trade_id, asset, kafka_partition, kafka_offset, processed_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(event.event_id)
        .bind(event.trade_id)
        .bind(&event.asset)
        .bind(kafka_partition)
        .bind(kafka_offset)
        .execute(&mut **tx)
        .await
        .context("claim_event insert")?;

        Ok(result.rows_affected() == 1)
    }

    // ── Portfolio state ───────────────────────────────────────────────────────

    /// Load the portfolio state for `asset` within the given transaction,
    /// acquiring a row-level lock (FOR UPDATE) so concurrent consumers cannot
    /// process the same asset simultaneously.
    ///
    /// If no row exists yet (first event for this asset), an empty
    /// `AssetPortfolio` is returned and the row is created to enable locking.
    pub async fn load_portfolio(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        asset: &str,
    ) -> Result<AssetPortfolio> {
        // Ensure the row exists so FOR UPDATE has something to lock.
        sqlx::query(
            r#"
            INSERT INTO portfolio_state (asset, lots, total_quantity, total_realized_pnl, updated_at)
            VALUES ($1, '[]'::jsonb, 0, 0, NOW())
            ON CONFLICT (asset) DO NOTHING
            "#,
        )
        .bind(asset)
        .execute(&mut **tx)
        .await
        .context("portfolio_state upsert for lock")?;

        let row = sqlx::query_as::<_, PortfolioRow>(
            r#"
            SELECT lots, total_quantity, total_realized_pnl
            FROM portfolio_state
            WHERE asset = $1
            FOR UPDATE
            "#,
        )
        .bind(asset)
        .fetch_one(&mut **tx)
        .await
        .context("portfolio_state select for update")?;

        let lots = serde_json::from_value(row.lots)
            .context("deserialize portfolio lots from JSONB")?;

        Ok(AssetPortfolio {
            asset: asset.to_owned(),
            lots,
            total_quantity: row.total_quantity,
            total_realized_pnl: row.total_realized_pnl,
        })
    }

    /// Persist the updated portfolio state within `tx`.
    pub async fn save_portfolio(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        portfolio: &AssetPortfolio,
        last_event_id: Uuid,
        last_trade_ts: DateTime<Utc>,
    ) -> Result<()> {
        let lots_json =
            serde_json::to_value(&portfolio.lots).context("serialize portfolio lots to JSONB")?;

        sqlx::query(
            r#"
            UPDATE portfolio_state
            SET
                lots               = $2,
                total_quantity     = $3,
                total_realized_pnl = $4,
                last_event_id      = $5,
                last_trade_ts      = $6,
                updated_at         = NOW()
            WHERE asset = $1
            "#,
        )
        .bind(&portfolio.asset)
        .bind(lots_json)
        .bind(portfolio.total_quantity)
        .bind(portfolio.total_realized_pnl)
        .bind(last_event_id)
        .bind(last_trade_ts)
        .execute(&mut **tx)
        .await
        .context("portfolio_state update")?;

        Ok(())
    }

    // ── PnL snapshots ─────────────────────────────────────────────────────────

    /// Write an immutable PnL snapshot for auditing / downstream consumers.
    pub async fn save_pnl_snapshot(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        snapshot_id: Uuid,
        event: &TradeEvent,
        realized_pnl: Decimal,
        total_realized_pnl: Decimal,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO pnl_snapshots
                (id, event_id, trade_id, asset, trade_side, trade_quantity, trade_price,
                 realized_pnl, total_realized_pnl, trade_ts, computed_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
            "#,
        )
        .bind(snapshot_id)
        .bind(event.event_id)
        .bind(event.trade_id)
        .bind(&event.asset)
        .bind(&event.side)
        .bind(event.quantity)
        .bind(event.price)
        .bind(realized_pnl)
        .bind(total_realized_pnl)
        .bind(event.timestamp)
        .execute(&mut **tx)
        .await
        .context("pnl_snapshots insert")?;

        Ok(())
    }
}
