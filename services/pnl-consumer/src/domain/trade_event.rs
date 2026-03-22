use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JSON payload published by the ledger-service to the `trades` Kafka topic.
/// Must match the `TradeEvent` struct in `ledger-service/src/infrastructure/producer.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeEvent {
    /// Unique event ID — the primary idempotency / deduplication key.
    pub event_id: Uuid,
    pub event_type: String,
    pub trade_id: Uuid,
    pub asset: String,
    pub quantity: Decimal,
    pub price: Decimal,
    /// "buy" or "sell" (lowercase)
    pub side: String,
    pub notional_value: Decimal,
    /// Realized PnL already computed by ledger-service (informational only).
    pub realized_pnl: Decimal,
    /// When the underlying trade was executed.
    pub timestamp: DateTime<Utc>,
    /// When the event was emitted.
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSide {
    Buy,
    Sell,
}

impl TradeEvent {
    pub fn side(&self) -> Option<TradeSide> {
        match self.side.to_lowercase().as_str() {
            "buy" => Some(TradeSide::Buy),
            "sell" => Some(TradeSide::Sell),
            _ => None,
        }
    }
}
