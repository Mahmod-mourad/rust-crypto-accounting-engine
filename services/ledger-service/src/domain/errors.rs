use rust_decimal::Decimal;
use thiserror::Error;

/// Domain errors specific to the trading and portfolio engine.
#[derive(Debug, Error, PartialEq)]
pub enum TradeError {
    #[error("insufficient balance for {asset}: required {required}, available {available}")]
    InsufficientBalance {
        asset: String,
        required: Decimal,
        available: Decimal,
    },

    #[error("invalid trade: {0}")]
    InvalidTrade(String),

    #[error("arithmetic overflow")]
    Overflow,

    #[error("persistence error: {0}")]
    Persistence(String),
}
