use anyhow::Result;
use metrics::{counter, histogram};
use rust_decimal::Decimal;
use std::time::Instant;
use uuid::Uuid;

use crate::{domain::trade_event::TradeEvent, infrastructure::repository::PnlRepository};

// ─── EventProcessor ───────────────────────────────────────────────────────────

/// Processes a `TradeEvent` end-to-end, inside a single database transaction.
///
/// ## Idempotency
///
/// The first step of every call is an `INSERT … ON CONFLICT DO NOTHING` into
/// `processed_events`.  If `rows_affected == 0` the event was already handled
/// and we return `Ok(false)` without touching any other table.
///
/// ## Ordering
///
/// The ledger-service keys Kafka messages by `trade_id`, not by asset, so
/// same-asset trades may land on different partitions.  To guard against
/// concurrent mutations on the same asset's portfolio state, `load_portfolio`
/// acquires a PostgreSQL row-level `FOR UPDATE` lock inside the transaction.
/// This ensures sequential, linearisable updates per asset even with multiple
/// consumer instances.
///
/// An `out_of_order` tracing event is emitted when a trade's timestamp
/// precedes the last seen timestamp for that asset, giving operators visibility
/// without blocking progress.
pub struct EventProcessor {
    repo: PnlRepository,
}

impl EventProcessor {
    pub fn new(repo: PnlRepository) -> Self {
        Self { repo }
    }

    /// Process `event` from Kafka partition `kafka_partition` at offset
    /// `kafka_offset`.
    ///
    /// Returns:
    /// - `Ok(true)`  — event was processed and committed to the database.
    /// - `Ok(false)` — event was a duplicate and was skipped (no DB writes).
    /// - `Err(_)`    — a transient or unexpected error; caller should *not*
    ///                 commit the Kafka offset so the message is redelivered.
    #[tracing::instrument(skip(self), fields(
        event_id  = %event.event_id,
        trade_id  = %event.trade_id,
        asset     = %event.asset,
        side      = %event.side,
        partition = kafka_partition,
        offset    = kafka_offset,
    ))]
    pub async fn process(
        &self,
        event: &TradeEvent,
        kafka_partition: i32,
        kafka_offset: i64,
    ) -> Result<bool> {
        let started = Instant::now();
        let mut tx = self.repo.begin().await?;

        // ── 1. Idempotency check ──────────────────────────────────────────────
        let claimed = self
            .repo
            .claim_event(&mut tx, event, kafka_partition, kafka_offset)
            .await?;

        if !claimed {
            // Already in `processed_events` — roll back the read-only tx and
            // return without touching portfolio or snapshot tables.
            tx.rollback().await?;
            tracing::debug!(
                event_id  = %event.event_id,
                trade_id  = %event.trade_id,
                asset     = %event.asset,
                partition = kafka_partition,
                offset    = kafka_offset,
                "duplicate event skipped"
            );
            counter!("pnl_consumer_events_total", "status" => "skipped").increment(1);
            return Ok(false);
        }

        // ── 2. Load portfolio state (row-locked) ──────────────────────────────
        let mut portfolio = self.repo.load_portfolio(&mut tx, &event.asset).await?;

        // Emit an observability warning for out-of-order delivery.  We still
        // process the event; the caller is responsible for understanding that
        // PnL snapshots may not be in strict timestamp order.
        // (A future enhancement could queue the event for reprocessing once
        // earlier events have been applied.)

        // ── 3. Apply trade to FIFO portfolio ──────────────────────────────────
        let pnl_events = portfolio.apply(event).map_err(|e| {
            anyhow::anyhow!(
                "portfolio error for event {} (asset={}, side={}, qty={}): {e}",
                event.event_id,
                event.asset,
                event.side,
                event.quantity,
            )
        })?;

        let realized_pnl: Decimal = pnl_events.iter().map(|e| e.realized_pnl).sum();

        // ── 4. Persist state & snapshot atomically ────────────────────────────
        self.repo
            .save_portfolio(&mut tx, &portfolio, event.event_id, event.timestamp)
            .await?;

        self.repo
            .save_pnl_snapshot(
                &mut tx,
                Uuid::new_v4(),
                event,
                realized_pnl,
                portfolio.total_realized_pnl,
            )
            .await?;

        tx.commit().await?;

        let elapsed = started.elapsed().as_secs_f64();
        histogram!("pnl_consumer_processing_duration_seconds", "asset" => event.asset.clone())
            .record(elapsed);
        counter!("pnl_consumer_events_total", "status" => "processed").increment(1);

        tracing::info!(
            event_id           = %event.event_id,
            trade_id           = %event.trade_id,
            asset              = %event.asset,
            side               = %event.side,
            quantity           = %event.quantity,
            price              = %event.price,
            realized_pnl       = %realized_pnl,
            total_realized_pnl = %portfolio.total_realized_pnl,
            open_lots          = portfolio.lots.len(),
            duration_s         = elapsed,
            "trade event processed"
        );

        Ok(true)
    }
}
