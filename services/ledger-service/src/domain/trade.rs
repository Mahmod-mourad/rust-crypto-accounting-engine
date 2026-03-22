use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::errors::TradeError;

/// Direction of a trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSide {
    Buy,
    Sell,
}

/// A single crypto trade event.
#[derive(Debug, Clone)]
pub struct Trade {
    pub id: Uuid,
    pub asset: String,
    pub quantity: Decimal,
    pub price: Decimal,
    pub timestamp: DateTime<Utc>,
    pub side: TradeSide,
}

impl Trade {
    /// Construct a validated trade. Quantity and price must be strictly positive.
    pub fn new(
        asset: impl Into<String>,
        quantity: Decimal,
        price: Decimal,
        timestamp: DateTime<Utc>,
        side: TradeSide,
    ) -> Result<Self, TradeError> {
        if quantity <= Decimal::ZERO {
            return Err(TradeError::InvalidTrade(
                "quantity must be positive".into(),
            ));
        }
        if price <= Decimal::ZERO {
            return Err(TradeError::InvalidTrade("price must be positive".into()));
        }
        Ok(Self {
            id: Uuid::new_v4(),
            asset: asset.into(),
            quantity,
            price,
            timestamp,
            side,
        })
    }

    /// Notional value of this trade (quantity × price).
    pub fn notional_value(&self) -> Decimal {
        self.quantity * self.price
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;

    fn ts() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    #[test]
    fn valid_buy_trade() {
        let t = Trade::new("BTC", dec!(1.5), dec!(30000), ts(), TradeSide::Buy).unwrap();
        assert_eq!(t.asset, "BTC");
        assert_eq!(t.notional_value(), dec!(45000));
    }

    #[test]
    fn valid_sell_trade() {
        let t = Trade::new("ETH", dec!(2), dec!(2000), ts(), TradeSide::Sell).unwrap();
        assert_eq!(t.side, TradeSide::Sell);
        assert_eq!(t.notional_value(), dec!(4000));
    }

    #[test]
    fn zero_quantity_rejected() {
        let err = Trade::new("BTC", dec!(0), dec!(30000), ts(), TradeSide::Buy).unwrap_err();
        assert!(matches!(err, TradeError::InvalidTrade(_)));
    }

    #[test]
    fn negative_quantity_rejected() {
        let err = Trade::new("BTC", dec!(-1), dec!(30000), ts(), TradeSide::Buy).unwrap_err();
        assert!(matches!(err, TradeError::InvalidTrade(_)));
    }

    #[test]
    fn zero_price_rejected() {
        let err = Trade::new("BTC", dec!(1), dec!(0), ts(), TradeSide::Buy).unwrap_err();
        assert!(matches!(err, TradeError::InvalidTrade(_)));
    }
}
