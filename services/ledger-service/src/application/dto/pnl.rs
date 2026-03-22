use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Caller-supplied current market prices, keyed by asset symbol.
///
/// All open positions in the portfolio must have a corresponding price entry;
/// otherwise the use case returns an error.
#[derive(Debug, Clone, Deserialize)]
pub struct PnLRequest {
    pub prices: HashMap<String, Decimal>,
}

/// PnL summary returned by `GetPnL`.
#[derive(Debug, Clone, Serialize)]
pub struct PnLResponse {
    /// Cumulative realized PnL (from closed lots).
    pub total_realized: Decimal,
    /// Mark-to-market unrealized PnL (from open lots at supplied prices).
    pub total_unrealized: Decimal,
    /// total_realized + total_unrealized.
    pub total: Decimal,
}
