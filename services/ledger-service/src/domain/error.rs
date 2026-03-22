use thiserror::Error;
use uuid::Uuid;

/// All domain-level errors for the ledger service.
/// These represent business rule violations and invariant failures.
#[derive(Debug, Error)]
pub enum DomainError {
    #[error("account not found: {0}")]
    AccountNotFound(Uuid),

    #[error("transaction not found: {0}")]
    TransactionNotFound(Uuid),

    #[error("insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: i64, available: i64 },

    #[error("invalid currency pair: {from} -> {to}")]
    InvalidCurrencyPair { from: String, to: String },

    #[error("duplicate transaction: {0}")]
    DuplicateTransaction(Uuid),
}
