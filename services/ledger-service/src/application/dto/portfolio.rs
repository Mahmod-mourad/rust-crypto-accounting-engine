use rust_decimal::Decimal;
use serde::Serialize;

/// Snapshot of a single open position.
#[derive(Debug, Clone, Serialize)]
pub struct PositionResponse {
    pub asset: String,
    pub quantity: Decimal,
    /// Weighted-average cost per unit across all open lots; `None` when flat.
    pub average_cost: Option<Decimal>,
}

/// Full portfolio snapshot returned by `GetPortfolio`.
#[derive(Debug, Clone, Serialize)]
pub struct PortfolioResponse {
    /// Open positions with a non-zero quantity, sorted by asset symbol.
    pub positions: Vec<PositionResponse>,
    /// Cumulative realized PnL across all trades processed so far.
    pub total_realized_pnl: Decimal,
}
