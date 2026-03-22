use rust_decimal::Decimal;

/// PnL realized from a single sell consuming one FIFO lot (partial or full).
#[derive(Debug, Clone)]
pub struct RealizedPnL {
    pub asset: String,
    pub quantity: Decimal,
    pub cost_basis: Decimal,
    pub proceeds: Decimal,
    pub realized_pnl: Decimal,
}
