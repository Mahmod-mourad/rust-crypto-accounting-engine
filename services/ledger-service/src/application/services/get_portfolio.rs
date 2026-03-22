use std::sync::{Arc, RwLock};

use rust_decimal::Decimal;

use crate::{
    application::{
        dto::portfolio::{PortfolioResponse, PositionResponse},
        error::AppResult,
    },
    domain::portfolio::Portfolio,
};

/// Returns a snapshot of the current portfolio state.
pub struct GetPortfolioService {
    portfolio: Arc<RwLock<Portfolio>>,
}

impl GetPortfolioService {
    pub fn new(portfolio: Arc<RwLock<Portfolio>>) -> Self {
        Self { portfolio }
    }

    /// Reads the portfolio and transforms it into a `PortfolioResponse` DTO.
    /// Only positions with a non-zero quantity are included.
    pub fn execute(&self) -> AppResult<PortfolioResponse> {
        let portfolio = self.portfolio.read().expect("portfolio lock poisoned");

        let mut positions: Vec<PositionResponse> = portfolio
            .positions()
            .values()
            .filter(|p| p.total_quantity > Decimal::ZERO)
            .map(|p| PositionResponse {
                asset: p.asset.clone(),
                quantity: p.total_quantity,
                average_cost: p.average_cost(),
            })
            .collect();

        // Stable, deterministic ordering for API responses.
        positions.sort_by(|a, b| a.asset.cmp(&b.asset));

        Ok(PortfolioResponse {
            positions,
            total_realized_pnl: portfolio.total_realized_pnl,
        })
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use rust_decimal_macros::dec;

    use super::*;
    use crate::domain::trade::{Trade, TradeSide};

    fn portfolio_with_trades() -> Arc<RwLock<Portfolio>> {
        let p = Arc::new(RwLock::new(Portfolio::new()));
        {
            let mut portfolio = p.write().unwrap();
            let buy_btc =
                Trade::new("BTC", dec!(2), dec!(40_000), Utc::now(), TradeSide::Buy).unwrap();
            let buy_eth =
                Trade::new("ETH", dec!(5), dec!(2_000), Utc::now(), TradeSide::Buy).unwrap();
            let sell_btc =
                Trade::new("BTC", dec!(1), dec!(50_000), Utc::now(), TradeSide::Sell).unwrap();

            portfolio.apply_trade(&buy_btc).unwrap();
            portfolio.apply_trade(&buy_eth).unwrap();
            portfolio.apply_trade(&sell_btc).unwrap();
        }
        p
    }

    #[test]
    fn returns_open_positions_only() {
        let svc = GetPortfolioService::new(portfolio_with_trades());
        let resp = svc.execute().unwrap();

        // Both BTC (1 remaining) and ETH (5) should appear.
        assert_eq!(resp.positions.len(), 2);
    }

    #[test]
    fn positions_sorted_alphabetically() {
        let svc = GetPortfolioService::new(portfolio_with_trades());
        let resp = svc.execute().unwrap();

        let assets: Vec<&str> = resp.positions.iter().map(|p| p.asset.as_str()).collect();
        assert_eq!(assets, vec!["BTC", "ETH"]);
    }

    #[test]
    fn realized_pnl_is_populated() {
        let svc = GetPortfolioService::new(portfolio_with_trades());
        let resp = svc.execute().unwrap();

        // 1 BTC sold: ($50_000 − $40_000) × 1 = $10_000
        assert_eq!(resp.total_realized_pnl, dec!(10_000));
    }

    #[test]
    fn flat_positions_are_excluded() {
        let p = Arc::new(RwLock::new(Portfolio::new()));
        {
            let mut portfolio = p.write().unwrap();
            let buy = Trade::new("BTC", dec!(1), dec!(100), Utc::now(), TradeSide::Buy).unwrap();
            let sell = Trade::new("BTC", dec!(1), dec!(200), Utc::now(), TradeSide::Sell).unwrap();
            portfolio.apply_trade(&buy).unwrap();
            portfolio.apply_trade(&sell).unwrap();
        }

        let svc = GetPortfolioService::new(p);
        let resp = svc.execute().unwrap();

        assert!(resp.positions.is_empty());
    }

    #[test]
    fn average_cost_is_correct() {
        let p = Arc::new(RwLock::new(Portfolio::new()));
        {
            let mut portfolio = p.write().unwrap();
            // Buy 4 @ $100 and 6 @ $200 → avg = (400 + 1200) / 10 = $160
            let b1 = Trade::new("BTC", dec!(4), dec!(100), Utc::now(), TradeSide::Buy).unwrap();
            let b2 = Trade::new("BTC", dec!(6), dec!(200), Utc::now(), TradeSide::Buy).unwrap();
            portfolio.apply_trade(&b1).unwrap();
            portfolio.apply_trade(&b2).unwrap();
        }

        let svc = GetPortfolioService::new(p);
        let resp = svc.execute().unwrap();

        let btc = resp.positions.iter().find(|p| p.asset == "BTC").unwrap();
        assert_eq!(btc.average_cost, Some(dec!(160)));
    }
}
