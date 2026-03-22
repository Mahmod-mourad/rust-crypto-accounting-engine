use std::collections::{HashMap, VecDeque};

use rust_decimal::Decimal;

use super::{
    errors::TradeError,
    pnl::{PricingEngine, RealizedPnL, UnrealizedPnL},
    trade::{Trade, TradeSide},
};

// ─── Lot ─────────────────────────────────────────────────────────────────────

/// A single buy lot used for FIFO cost-basis tracking.
#[derive(Debug, Clone, PartialEq)]
pub struct Lot {
    /// Remaining units in this lot.
    pub quantity: Decimal,
    /// Price paid per unit when this lot was acquired.
    pub cost_per_unit: Decimal,
}

// ─── Position ────────────────────────────────────────────────────────────────

/// Per-asset position: a FIFO queue of open lots.
#[derive(Debug, Clone)]
pub struct Position {
    pub asset: String,
    /// Oldest lot at the front; newest at the back.
    pub lots: VecDeque<Lot>,
    /// Sum of all lot quantities — kept in sync for O(1) balance checks.
    pub total_quantity: Decimal,
}

impl Position {
    pub fn new(asset: impl Into<String>) -> Self {
        Self {
            asset: asset.into(),
            lots: VecDeque::new(),
            total_quantity: Decimal::ZERO,
        }
    }

    /// Push a new buy lot onto the back of the queue.
    pub fn add_lot(&mut self, quantity: Decimal, cost_per_unit: Decimal) {
        self.lots.push_back(Lot { quantity, cost_per_unit });
        self.total_quantity += quantity;
    }

    /// Consume `sell_quantity` units FIFO and return one `RealizedPnL` event
    /// per lot touched. Returns `InsufficientBalance` if the position is too
    /// small.
    pub fn consume_fifo(
        &mut self,
        sell_quantity: Decimal,
        sell_price: Decimal,
    ) -> Result<Vec<RealizedPnL>, TradeError> {
        if sell_quantity > self.total_quantity {
            return Err(TradeError::InsufficientBalance {
                asset: self.asset.clone(),
                required: sell_quantity,
                available: self.total_quantity,
            });
        }

        let mut remaining = sell_quantity;
        let mut events = Vec::new();

        while remaining > Decimal::ZERO {
            // Safety: we verified total_quantity ≥ sell_quantity above.
            let lot = self.lots.front_mut().expect("lot queue unexpectedly empty");

            let consumed = remaining.min(lot.quantity);
            let cost_basis = consumed * lot.cost_per_unit;
            let proceeds = consumed * sell_price;

            events.push(RealizedPnL {
                asset: self.asset.clone(),
                quantity: consumed,
                cost_basis,
                proceeds,
                realized_pnl: proceeds - cost_basis,
            });

            lot.quantity -= consumed;
            self.total_quantity -= consumed;
            remaining -= consumed;

            if lot.quantity == Decimal::ZERO {
                self.lots.pop_front();
            }
        }

        Ok(events)
    }

    /// Unrealized PnL for this position at `current_price`.
    pub fn unrealized_pnl(&self, current_price: Decimal) -> UnrealizedPnL {
        let cost_basis: Decimal =
            self.lots.iter().map(|l| l.quantity * l.cost_per_unit).sum();
        let market_value = self.total_quantity * current_price;
        UnrealizedPnL {
            asset: self.asset.clone(),
            quantity: self.total_quantity,
            cost_basis,
            market_value,
            unrealized_pnl: market_value - cost_basis,
        }
    }

    /// Weighted-average cost per unit across all open lots.
    /// Returns `None` when the position is empty.
    pub fn average_cost(&self) -> Option<Decimal> {
        if self.total_quantity == Decimal::ZERO {
            return None;
        }
        let total_cost: Decimal =
            self.lots.iter().map(|l| l.quantity * l.cost_per_unit).sum();
        Some(total_cost / self.total_quantity)
    }
}

// ─── Portfolio ───────────────────────────────────────────────────────────────

/// A multi-asset portfolio. Applies trades and tracks positions.
#[derive(Debug, Default)]
pub struct Portfolio {
    positions: HashMap<String, Position>,
    /// Cumulative realized PnL across all trades ever processed.
    pub total_realized_pnl: Decimal,
}

impl Portfolio {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a trade. Returns the `RealizedPnL` events produced (empty for
    /// buys, one event per lot touched for sells).
    pub fn apply_trade(&mut self, trade: &Trade) -> Result<Vec<RealizedPnL>, TradeError> {
        match trade.side {
            TradeSide::Buy => {
                self.positions
                    .entry(trade.asset.clone())
                    .or_insert_with(|| Position::new(&trade.asset))
                    .add_lot(trade.quantity, trade.price);
                Ok(vec![])
            }
            TradeSide::Sell => {
                let position =
                    self.positions.get_mut(&trade.asset).ok_or_else(|| {
                        TradeError::InsufficientBalance {
                            asset: trade.asset.clone(),
                            required: trade.quantity,
                            available: Decimal::ZERO,
                        }
                    })?;

                let events = position.consume_fifo(trade.quantity, trade.price)?;

                // Accumulate into the portfolio total.
                for e in &events {
                    self.total_realized_pnl += e.realized_pnl;
                }

                Ok(events)
            }
        }
    }

    pub fn get_position(&self, asset: &str) -> Option<&Position> {
        self.positions.get(asset)
    }

    pub fn positions(&self) -> &HashMap<String, Position> {
        &self.positions
    }

    /// Total unrealized PnL across all open positions using the supplied
    /// pricing engine. Returns `InvalidTrade` if any asset has no price.
    pub fn total_unrealized_pnl(
        &self,
        engine: &dyn PricingEngine,
    ) -> Result<Decimal, TradeError> {
        let mut total = Decimal::ZERO;
        for (asset, position) in &self.positions {
            if position.total_quantity == Decimal::ZERO {
                continue;
            }
            let price = engine.get_price(asset).ok_or_else(|| {
                TradeError::InvalidTrade(format!("no market price for asset '{asset}'"))
            })?;
            total += position.unrealized_pnl(price).unrealized_pnl;
        }
        Ok(total)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use rust_decimal_macros::dec;

    use crate::domain::{
        pnl::StaticPricingEngine,
        trade::{Trade, TradeSide},
    };

    use super::*;

    fn ts() -> DateTime<chrono::Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn buy(asset: &str, qty: Decimal, price: Decimal) -> Trade {
        Trade::new(asset, qty, price, ts(), TradeSide::Buy).unwrap()
    }

    fn sell(asset: &str, qty: Decimal, price: Decimal) -> Trade {
        Trade::new(asset, qty, price, ts(), TradeSide::Sell).unwrap()
    }

    // ── Lot / Position ────────────────────────────────────────────────────────

    #[test]
    fn add_single_lot() {
        let mut pos = Position::new("BTC");
        pos.add_lot(dec!(10), dec!(100));
        assert_eq!(pos.total_quantity, dec!(10));
        assert_eq!(pos.lots.len(), 1);
    }

    #[test]
    fn average_cost_empty_position() {
        let pos = Position::new("BTC");
        assert_eq!(pos.average_cost(), None);
    }

    #[test]
    fn average_cost_single_lot() {
        let mut pos = Position::new("BTC");
        pos.add_lot(dec!(4), dec!(100));
        assert_eq!(pos.average_cost(), Some(dec!(100)));
    }

    #[test]
    fn average_cost_two_lots() {
        let mut pos = Position::new("BTC");
        pos.add_lot(dec!(4), dec!(100)); // cost 400
        pos.add_lot(dec!(6), dec!(200)); // cost 1200
        // avg = 1600 / 10 = 160
        assert_eq!(pos.average_cost(), Some(dec!(160)));
    }

    // ── FIFO correctness ──────────────────────────────────────────────────────

    #[test]
    fn fifo_sell_within_first_lot() {
        // Buy 10 @ $100 → sell 8 @ $150
        // Realized: 8 × ($150 − $100) = $400
        let mut pos = Position::new("BTC");
        pos.add_lot(dec!(10), dec!(100));

        let events = pos.consume_fifo(dec!(8), dec!(150)).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].quantity, dec!(8));
        assert_eq!(events[0].cost_basis, dec!(800));
        assert_eq!(events[0].proceeds, dec!(1200));
        assert_eq!(events[0].realized_pnl, dec!(400));

        assert_eq!(pos.total_quantity, dec!(2));
        assert_eq!(pos.lots.len(), 1);
        assert_eq!(pos.lots[0].quantity, dec!(2));
    }

    #[test]
    fn fifo_exact_lot_consumed() {
        // Buy 5 @ $200 → sell exactly 5 @ $300
        let mut pos = Position::new("ETH");
        pos.add_lot(dec!(5), dec!(200));

        let events = pos.consume_fifo(dec!(5), dec!(300)).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].realized_pnl, dec!(500));
        assert_eq!(pos.total_quantity, dec!(0));
        assert!(pos.lots.is_empty());
    }

    #[test]
    fn fifo_sell_spans_two_lots() {
        // Buy 3 @ $100 (Lot A), then 5 @ $200 (Lot B)
        // Sell 5 @ $300:
        //   From Lot A: 3 × ($300 − $100) = $600
        //   From Lot B: 2 × ($300 − $200) = $200
        //   Total realized: $800
        let mut pos = Position::new("BTC");
        pos.add_lot(dec!(3), dec!(100));
        pos.add_lot(dec!(5), dec!(200));

        let events = pos.consume_fifo(dec!(5), dec!(300)).unwrap();

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].quantity, dec!(3));
        assert_eq!(events[0].realized_pnl, dec!(600));
        assert_eq!(events[1].quantity, dec!(2));
        assert_eq!(events[1].realized_pnl, dec!(200));

        let total_realized: Decimal = events.iter().map(|e| e.realized_pnl).sum();
        assert_eq!(total_realized, dec!(800));

        // Lot B still has 3 remaining.
        assert_eq!(pos.total_quantity, dec!(3));
        assert_eq!(pos.lots.len(), 1);
        assert_eq!(pos.lots[0].cost_per_unit, dec!(200));
        assert_eq!(pos.lots[0].quantity, dec!(3));
    }

    #[test]
    fn fifo_sell_spans_three_lots() {
        let mut pos = Position::new("BTC");
        pos.add_lot(dec!(2), dec!(10));
        pos.add_lot(dec!(3), dec!(20));
        pos.add_lot(dec!(5), dec!(30));

        // Sell 7 @ $40:
        //   Lot1: 2 × (40−10) = 60
        //   Lot2: 3 × (40−20) = 60
        //   Lot3: 2 × (40−30) = 20  (2 of 5 consumed)
        let events = pos.consume_fifo(dec!(7), dec!(40)).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].realized_pnl, dec!(60));
        assert_eq!(events[1].realized_pnl, dec!(60));
        assert_eq!(events[2].realized_pnl, dec!(20));

        assert_eq!(pos.total_quantity, dec!(3)); // 3 remaining in Lot3
    }

    // ── Partial sell ──────────────────────────────────────────────────────────

    #[test]
    fn partial_sell_leaves_remainder() {
        let mut pos = Position::new("SOL");
        pos.add_lot(dec!(100), dec!(20));

        pos.consume_fifo(dec!(1), dec!(25)).unwrap();

        assert_eq!(pos.total_quantity, dec!(99));
        assert_eq!(pos.lots[0].quantity, dec!(99));
    }

    // ── Error cases ───────────────────────────────────────────────────────────

    #[test]
    fn sell_more_than_held_returns_error() {
        let mut pos = Position::new("BTC");
        pos.add_lot(dec!(5), dec!(100));

        let err = pos.consume_fifo(dec!(10), dec!(200)).unwrap_err();
        assert_eq!(
            err,
            TradeError::InsufficientBalance {
                asset: "BTC".into(),
                required: dec!(10),
                available: dec!(5),
            }
        );
        // Position must be unchanged after error.
        assert_eq!(pos.total_quantity, dec!(5));
    }

    #[test]
    fn sell_with_no_position_returns_error() {
        let mut portfolio = Portfolio::new();
        let t = sell("BTC", dec!(1), dec!(50000));
        let err = portfolio.apply_trade(&t).unwrap_err();
        assert!(matches!(err, TradeError::InsufficientBalance { .. }));
    }

    // ── Unrealized PnL ────────────────────────────────────────────────────────

    #[test]
    fn unrealized_pnl_single_lot() {
        let mut pos = Position::new("BTC");
        pos.add_lot(dec!(2), dec!(100));
        let upnl = pos.unrealized_pnl(dec!(150));
        assert_eq!(upnl.unrealized_pnl, dec!(100)); // 2 × (150−100)
        assert_eq!(upnl.cost_basis, dec!(200));
        assert_eq!(upnl.market_value, dec!(300));
    }

    #[test]
    fn unrealized_pnl_is_negative_when_underwater() {
        let mut pos = Position::new("ETH");
        pos.add_lot(dec!(10), dec!(500));
        let upnl = pos.unrealized_pnl(dec!(400)); // price fell
        assert_eq!(upnl.unrealized_pnl, dec!(-1000));
    }

    // ── Portfolio: multiple assets ────────────────────────────────────────────

    #[test]
    fn multiple_assets_tracked_independently() {
        let mut p = Portfolio::new();

        p.apply_trade(&buy("BTC", dec!(1), dec!(40000))).unwrap();
        p.apply_trade(&buy("ETH", dec!(5), dec!(2000))).unwrap();
        p.apply_trade(&buy("BTC", dec!(1), dec!(45000))).unwrap();

        assert_eq!(p.get_position("BTC").unwrap().total_quantity, dec!(2));
        assert_eq!(p.get_position("ETH").unwrap().total_quantity, dec!(5));
    }

    #[test]
    fn realized_pnl_accumulated_in_portfolio() {
        let mut p = Portfolio::new();

        p.apply_trade(&buy("BTC", dec!(10), dec!(100))).unwrap();
        p.apply_trade(&sell("BTC", dec!(5), dec!(200))).unwrap(); // profit $500
        p.apply_trade(&sell("BTC", dec!(5), dec!(50))).unwrap();  // loss $250

        // Net: $500 − $250 = $250
        assert_eq!(p.total_realized_pnl, dec!(250));
    }

    #[test]
    fn portfolio_total_unrealized_pnl_via_engine() {
        let mut p = Portfolio::new();
        p.apply_trade(&buy("BTC", dec!(2), dec!(100))).unwrap();
        p.apply_trade(&buy("ETH", dec!(4), dec!(50))).unwrap();

        let mut prices = std::collections::HashMap::new();
        prices.insert("BTC".into(), dec!(200));
        prices.insert("ETH".into(), dec!(40));
        let engine = StaticPricingEngine::new(prices);

        // BTC unrealized: 2 × (200−100) = 200
        // ETH unrealized: 4 × (40−50) = −40
        // total = 160
        let total = p.total_unrealized_pnl(&engine).unwrap();
        assert_eq!(total, dec!(160));
    }

    #[test]
    fn missing_price_for_asset_returns_error() {
        let mut p = Portfolio::new();
        p.apply_trade(&buy("BTC", dec!(1), dec!(100))).unwrap();

        let engine = StaticPricingEngine::new(std::collections::HashMap::new());
        let err = p.total_unrealized_pnl(&engine).unwrap_err();
        assert!(matches!(err, TradeError::InvalidTrade(_)));
    }

    // ── Edge cases ────────────────────────────────────────────────────────────

    #[test]
    fn zero_realized_pnl_when_price_unchanged() {
        let mut pos = Position::new("BTC");
        pos.add_lot(dec!(3), dec!(1000));
        let events = pos.consume_fifo(dec!(3), dec!(1000)).unwrap();
        assert_eq!(events[0].realized_pnl, dec!(0));
    }

    #[test]
    fn large_numbers_no_overflow() {
        let mut pos = Position::new("BTC");
        // 10^12 units at $10^6 per unit
        pos.add_lot(dec!(1_000_000_000_000), dec!(1_000_000));
        let events = pos
            .consume_fifo(dec!(1_000_000_000_000), dec!(2_000_000))
            .unwrap();
        // Realized: 10^12 × $10^6 = $10^18
        assert_eq!(events[0].realized_pnl, dec!(1_000_000_000_000_000_000));
    }

    #[test]
    fn very_small_fractional_quantity() {
        let mut pos = Position::new("BTC");
        pos.add_lot(dec!(0.00000001), dec!(50000));
        let events = pos.consume_fifo(dec!(0.00000001), dec!(60000)).unwrap();
        // profit: 0.00000001 × $10000 = $0.0001
        assert_eq!(events[0].realized_pnl, dec!(0.0001));
    }

    #[test]
    fn buy_sell_buy_sell_sequence_fifo_order() {
        // Ensures lot ordering is correct across interleaved trades.
        let mut p = Portfolio::new();

        p.apply_trade(&buy("ETH", dec!(10), dec!(100))).unwrap(); // Lot1: 10@100
        p.apply_trade(&buy("ETH", dec!(10), dec!(200))).unwrap(); // Lot2: 10@200

        // Sell 10: should consume Lot1 first
        let events = p.apply_trade(&sell("ETH", dec!(10), dec!(300))).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].cost_basis, dec!(1000)); // from Lot1 @ $100

        // Sell 5 more: should now consume from Lot2
        let events2 = p.apply_trade(&sell("ETH", dec!(5), dec!(300))).unwrap();
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].cost_basis, dec!(1000)); // 5 × $200

        let pos = p.get_position("ETH").unwrap();
        assert_eq!(pos.total_quantity, dec!(5));
        assert_eq!(pos.lots[0].cost_per_unit, dec!(200));
    }
}
