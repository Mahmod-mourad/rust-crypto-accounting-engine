use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Inbound request to create a new trade.
#[derive(Debug, Clone, Deserialize)]
pub struct TradeRequest {
    /// Asset symbol, e.g. "BTC", "ETH".
    pub asset: String,
    /// Number of units to trade — must be strictly positive.
    pub quantity: Decimal,
    /// Price per unit — must be strictly positive.
    pub price: Decimal,
    /// Direction of the trade.
    pub side: TradeSideRequest,
    /// Execution timestamp; defaults to `Utc::now()` when absent.
    pub timestamp: Option<DateTime<Utc>>,
}

/// Serde-friendly representation of trade direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeSideRequest {
    Buy,
    Sell,
}

/// Outbound DTO returned after a trade is successfully applied.
#[derive(Debug, Clone, Serialize)]
pub struct TradeResponse {
    pub id: Uuid,
    pub asset: String,
    pub quantity: Decimal,
    pub price: Decimal,
    /// "buy" or "sell".
    pub side: String,
    /// quantity × price.
    pub notional_value: Decimal,
    pub timestamp: DateTime<Utc>,
    /// Sum of realized PnL events produced by this trade (non-zero for sells).
    pub realized_pnl: Decimal,
}
