use std::collections::VecDeque;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{pnl::RealizedPnL, trade_event::{TradeEvent, TradeSide}};

// ─── Lot ─────────────────────────────────────────────────────────────────────

/// A single buy lot — serializable to JSONB for DB persistence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lot {
    pub quantity: Decimal,
    pub cost_per_unit: Decimal,
}

// ─── AssetPortfolio ───────────────────────────────────────────────────────────

/// Per-asset FIFO portfolio state.  Persisted in `portfolio_state` as JSONB.
/// Loaded and saved atomically within the processing transaction so state
/// is always consistent with `processed_events`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetPortfolio {
    pub asset: String,
    /// Oldest lot at the front; newest at the back.
    pub lots: VecDeque<Lot>,
    pub total_quantity: Decimal,
    /// Cumulative realized PnL for this asset across all processed sells.
    pub total_realized_pnl: Decimal,
}

impl AssetPortfolio {
    pub fn new(asset: impl Into<String>) -> Self {
        Self {
            asset: asset.into(),
            lots: VecDeque::new(),
            total_quantity: Decimal::ZERO,
            total_realized_pnl: Decimal::ZERO,
        }
    }

    /// Apply a `TradeEvent` to this portfolio.
    ///
    /// - **Buy**: appends a new lot; returns an empty `RealizedPnL` vec.
    /// - **Sell**: FIFO-matches against open lots; accumulates `total_realized_pnl`.
    ///
    /// Returns `PortfolioError::InsufficientBalance` if a sell exceeds the
    /// current position, or `PortfolioError::UnknownSide` for unrecognized sides.
    pub fn apply(&mut self, event: &TradeEvent) -> Result<Vec<RealizedPnL>, PortfolioError> {
        let side = event
            .side()
            .ok_or_else(|| PortfolioError::UnknownSide(event.side.clone()))?;

        match side {
            TradeSide::Buy => {
                self.lots.push_back(Lot {
                    quantity: event.quantity,
                    cost_per_unit: event.price,
                });
                self.total_quantity += event.quantity;
                Ok(vec![])
            }
            TradeSide::Sell => {
                if event.quantity > self.total_quantity {
                    return Err(PortfolioError::InsufficientBalance {
                        asset: self.asset.clone(),
                        required: event.quantity,
                        available: self.total_quantity,
                    });
                }

                let pnl_events = self.consume_fifo(event.quantity, event.price);

                for e in &pnl_events {
                    self.total_realized_pnl += e.realized_pnl;
                }

                Ok(pnl_events)
            }
        }
    }

    /// Consume `sell_qty` units from the front of the FIFO lot queue.
    ///
    /// Caller must verify `sell_qty <= total_quantity` beforehand.
    fn consume_fifo(&mut self, mut sell_qty: Decimal, sell_price: Decimal) -> Vec<RealizedPnL> {
        let mut events = Vec::new();

        while sell_qty > Decimal::ZERO {
            // Safety: caller guarantees total_quantity >= sell_qty.
            let lot = self.lots.front_mut().expect("lot queue unexpectedly empty");

            let consumed = sell_qty.min(lot.quantity);
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
            sell_qty -= consumed;

            if lot.quantity == Decimal::ZERO {
                self.lots.pop_front();
            }
        }

        events
    }
}

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum PortfolioError {
    #[error("insufficient balance for {asset}: required {required}, available {available}")]
    InsufficientBalance {
        asset: String,
        required: Decimal,
        available: Decimal,
    },
    #[error("unknown trade side: '{0}'")]
    UnknownSide(String),
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    use super::*;

    fn event(side: &str, qty: Decimal, price: Decimal) -> TradeEvent {
        TradeEvent {
            event_id: Uuid::new_v4(),
            event_type: "trade_created".into(),
            trade_id: Uuid::new_v4(),
            asset: "BTC".into(),
            quantity: qty,
            price,
            side: side.into(),
            notional_value: qty * price,
            realized_pnl: Decimal::ZERO,
            timestamp: Utc::now(),
            published_at: Utc::now(),
        }
    }

    #[test]
    fn buy_adds_lot() {
        let mut p = AssetPortfolio::new("BTC");
        p.apply(&event("buy", dec!(1), dec!(40000))).unwrap();
        assert_eq!(p.total_quantity, dec!(1));
        assert_eq!(p.lots.len(), 1);
    }

    #[test]
    fn sell_within_single_lot() {
        let mut p = AssetPortfolio::new("BTC");
        p.apply(&event("buy", dec!(10), dec!(100))).unwrap();
        let pnl = p.apply(&event("sell", dec!(8), dec!(150))).unwrap();
        // 8 × (150 − 100) = 400
        assert_eq!(pnl.len(), 1);
        assert_eq!(pnl[0].realized_pnl, dec!(400));
        assert_eq!(p.total_quantity, dec!(2));
        assert_eq!(p.total_realized_pnl, dec!(400));
    }

    #[test]
    fn sell_spans_two_lots() {
        let mut p = AssetPortfolio::new("BTC");
        p.apply(&event("buy", dec!(3), dec!(100))).unwrap();
        p.apply(&event("buy", dec!(5), dec!(200))).unwrap();
        // Sell 5 @ 300: 3×(300-100)=600 from Lot1, 2×(300-200)=200 from Lot2
        let pnl = p.apply(&event("sell", dec!(5), dec!(300))).unwrap();
        assert_eq!(pnl.len(), 2);
        let total: Decimal = pnl.iter().map(|e| e.realized_pnl).sum();
        assert_eq!(total, dec!(800));
        assert_eq!(p.total_realized_pnl, dec!(800));
        assert_eq!(p.total_quantity, dec!(3));
    }

    #[test]
    fn sell_exceeding_balance_returns_error() {
        let mut p = AssetPortfolio::new("BTC");
        p.apply(&event("buy", dec!(1), dec!(40000))).unwrap();
        let err = p.apply(&event("sell", dec!(2), dec!(50000))).unwrap_err();
        assert!(matches!(err, PortfolioError::InsufficientBalance { .. }));
        // Position unchanged after error
        assert_eq!(p.total_quantity, dec!(1));
    }

    #[test]
    fn unknown_side_returns_error() {
        let mut p = AssetPortfolio::new("BTC");
        let mut e = event("buy", dec!(1), dec!(100));
        e.side = "market".into();
        let err = p.apply(&e).unwrap_err();
        assert!(matches!(err, PortfolioError::UnknownSide(_)));
    }

    #[test]
    fn fifo_ordering_correct_across_sequence() {
        let mut p = AssetPortfolio::new("ETH");
        p.apply(&event("buy", dec!(10), dec!(100))).unwrap(); // Lot1 @ 100
        p.apply(&event("buy", dec!(10), dec!(200))).unwrap(); // Lot2 @ 200
        // Sell 10 — must consume Lot1 first
        let pnl = p.apply(&event("sell", dec!(10), dec!(300))).unwrap();
        assert_eq!(pnl[0].cost_basis, dec!(1000)); // from Lot1
        // Sell 5 more — now consuming Lot2
        let pnl2 = p.apply(&event("sell", dec!(5), dec!(300))).unwrap();
        assert_eq!(pnl2[0].cost_basis, dec!(1000)); // 5 × 200 from Lot2
    }
}
