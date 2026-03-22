use rust_decimal::Decimal;

/// A realized PnL event produced when a sell consumes one or more lots.
#[derive(Debug, Clone, PartialEq)]
pub struct RealizedPnL {
    /// Asset symbol (e.g. "BTC").
    pub asset: String,
    /// Units sold in this lot match.
    pub quantity: Decimal,
    /// Total cost basis of the consumed units (quantity × lot cost-per-unit).
    pub cost_basis: Decimal,
    /// Total proceeds from the sell (quantity × sell price).
    pub proceeds: Decimal,
    /// proceeds − cost_basis. Positive means profit.
    pub realized_pnl: Decimal,
}

/// Unrealized PnL snapshot for a live position.
#[derive(Debug, Clone, PartialEq)]
pub struct UnrealizedPnL {
    pub asset: String,
    pub quantity: Decimal,
    /// Total cost basis of open lots.
    pub cost_basis: Decimal,
    /// Mark-to-market value at current_price.
    pub market_value: Decimal,
    /// market_value − cost_basis.
    pub unrealized_pnl: Decimal,
}

/// Aggregated summary across all assets in a portfolio.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PnLSummary {
    pub total_realized: Decimal,
    pub total_unrealized: Decimal,
}

impl PnLSummary {
    pub fn total(&self) -> Decimal {
        self.total_realized + self.total_unrealized
    }
}

// ─── Pricing engine trait (bonus) ────────────────────────────────────────────

/// Abstraction over a market data source.
/// Implement this to plug in live prices, static fixtures, or mocks.
pub trait PricingEngine {
    /// Return the current price for `asset`, or `None` if unavailable.
    fn get_price(&self, asset: &str) -> Option<Decimal>;
}

/// A simple in-memory pricing engine backed by a static map — useful in tests.
pub struct StaticPricingEngine {
    prices: std::collections::HashMap<String, Decimal>,
}

impl StaticPricingEngine {
    pub fn new(prices: std::collections::HashMap<String, Decimal>) -> Self {
        Self { prices }
    }
}

impl PricingEngine for StaticPricingEngine {
    fn get_price(&self, asset: &str) -> Option<Decimal> {
        self.prices.get(asset).copied()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;

    #[test]
    fn pnl_summary_total() {
        let s = PnLSummary {
            total_realized: dec!(500),
            total_unrealized: dec!(200),
        };
        assert_eq!(s.total(), dec!(700));
    }

    #[test]
    fn pnl_summary_default_is_zero() {
        let s = PnLSummary::default();
        assert_eq!(s.total(), dec!(0));
    }

    #[test]
    fn static_pricing_engine() {
        let mut map = std::collections::HashMap::new();
        map.insert("BTC".into(), dec!(50000));
        map.insert("ETH".into(), dec!(3000));

        let engine = StaticPricingEngine::new(map);
        assert_eq!(engine.get_price("BTC"), Some(dec!(50000)));
        assert_eq!(engine.get_price("ETH"), Some(dec!(3000)));
        assert_eq!(engine.get_price("SOL"), None);
    }
}
