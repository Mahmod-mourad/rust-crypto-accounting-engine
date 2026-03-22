use async_trait::async_trait;
use uuid::Uuid;

use super::{
    error::DomainError,
    errors::TradeError,
    model::{Account, Transaction},
    trade::Trade,
};

/// Port: defines how the application layer accesses account persistence.
/// Implemented by the infrastructure layer.
pub trait AccountRepository: Send + Sync {
    fn find_by_id(
        &self,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<Account, DomainError>> + Send;

    fn save(
        &self,
        account: &Account,
    ) -> impl std::future::Future<Output = Result<(), DomainError>> + Send;
}

/// Port: defines how the application layer accesses transaction persistence.
/// Implemented by the infrastructure layer.
pub trait TransactionRepository: Send + Sync {
    fn find_by_id(
        &self,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<Transaction, DomainError>> + Send;

    fn save(
        &self,
        transaction: &Transaction,
    ) -> impl std::future::Future<Output = Result<(), DomainError>> + Send;
}

/// Port: defines how the application layer persists and loads trades.
/// Implemented by `PgTradeRepository` in the infrastructure layer.
///
/// Uses `async_trait` so the trait is object-safe (usable as `dyn TradeRepository`).
#[async_trait]
pub trait TradeRepository: Send + Sync {
    /// Persist a single trade. Idempotent: duplicate IDs are silently ignored.
    async fn save_trade(&self, trade: &Trade) -> Result<(), TradeError>;

    /// Load all trades, ordered by timestamp descending.
    async fn get_trades(&self) -> Result<Vec<Trade>, TradeError>;
}
