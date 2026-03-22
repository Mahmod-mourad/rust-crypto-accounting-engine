use std::sync::{Arc, RwLock};

use anyhow::Context;

use crate::{
    application::{
        dto::pnl::{PnLRequest, PnLResponse},
        error::AppResult,
    },
    domain::{pnl::StaticPricingEngine, portfolio::Portfolio},
};

/// Computes a full PnL summary — realized (from the portfolio) plus
/// unrealized (mark-to-market at caller-supplied prices).
pub struct GetPnLService {
    portfolio: Arc<RwLock<Portfolio>>,
}

impl GetPnLService {
    pub fn new(portfolio: Arc<RwLock<Portfolio>>) -> Self {
        Self { portfolio }
    }

    /// Returns a `PnLResponse` for the current portfolio state.
    ///
    /// The caller must supply a current market price for every asset that has
    /// an open position; otherwise an error is returned.
    pub fn execute(&self, req: PnLRequest) -> AppResult<PnLResponse> {
        let portfolio = self.portfolio.read().expect("portfolio lock poisoned");

        let engine = StaticPricingEngine::new(req.prices);

        let total_unrealized = portfolio
            .total_unrealized_pnl(&engine)
            .context("failed to compute unrealized PnL — ensure all open positions have a price")?;

        let total_realized = portfolio.total_realized_pnl;

        Ok(PnLResponse {
            total_realized,
            total_unrealized,
            total: total_realized + total_unrealized,
        })
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::Utc;
    use rust_decimal_macros::dec;

    use super::*;
    use crate::domain::trade::{Trade, TradeSide};

    fn make_portfolio() -> Arc<RwLock<Portfolio>> {
        let p = Arc::new(RwLock::new(Portfolio::new()));
        {
            let mut portfolio = p.write().unwrap();

            // Buy 2 BTC @ $40_000, sell 1 @ $50_000 → $10_000 realized
            let b1 =
                Trade::new("BTC", dec!(2), dec!(40_000), Utc::now(), TradeSide::Buy).unwrap();
            let s1 =
                Trade::new("BTC", dec!(1), dec!(50_000), Utc::now(), TradeSide::Sell).unwrap();
            // Buy 5 ETH @ $2_000 (all still open)
            let b2 = Trade::new("ETH", dec!(5), dec!(2_000), Utc::now(), TradeSide::Buy).unwrap();

            portfolio.apply_trade(&b1).unwrap();
            portfolio.apply_trade(&s1).unwrap();
            portfolio.apply_trade(&b2).unwrap();
        }
        p
    }

    fn prices(btc: rust_decimal::Decimal, eth: rust_decimal::Decimal) -> PnLRequest {
        let mut map = HashMap::new();
        map.insert("BTC".to_owned(), btc);
        map.insert("ETH".to_owned(), eth);
        PnLRequest { prices: map }
    }

    #[test]
    fn correct_realized_pnl() {
        let svc = GetPnLService::new(make_portfolio());
        let resp = svc
            .execute(prices(dec!(50_000), dec!(2_000)))
            .unwrap();
        assert_eq!(resp.total_realized, dec!(10_000));
    }

    #[test]
    fn correct_unrealized_pnl() {
        let svc = GetPnLService::new(make_portfolio());
        // 1 BTC open @ cost $40_000, current $45_000 → +$5_000
        // 5 ETH open @ cost $2_000, current $2_500 → +$2_500
        let resp = svc
            .execute(prices(dec!(45_000), dec!(2_500)))
            .unwrap();
        assert_eq!(resp.total_unrealized, dec!(7_500));
    }

    #[test]
    fn total_equals_realized_plus_unrealized() {
        let svc = GetPnLService::new(make_portfolio());
        let resp = svc
            .execute(prices(dec!(45_000), dec!(2_500)))
            .unwrap();
        assert_eq!(resp.total, resp.total_realized + resp.total_unrealized);
    }

    #[test]
    fn missing_price_returns_error() {
        let svc = GetPnLService::new(make_portfolio());
        // Only BTC price provided — ETH position has no price.
        let mut map = HashMap::new();
        map.insert("BTC".to_owned(), dec!(45_000));
        let err = svc.execute(PnLRequest { prices: map });
        assert!(err.is_err());
    }

    #[test]
    fn empty_portfolio_returns_zeros() {
        let p = Arc::new(RwLock::new(Portfolio::new()));
        let svc = GetPnLService::new(p);
        let resp = svc.execute(PnLRequest { prices: HashMap::new() }).unwrap();
        assert_eq!(resp.total_realized, dec!(0));
        assert_eq!(resp.total_unrealized, dec!(0));
        assert_eq!(resp.total, dec!(0));
    }
}
