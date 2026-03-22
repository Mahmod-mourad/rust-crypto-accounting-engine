use std::sync::{Arc, RwLock};

use crate::{
    application::services::{CreateTradeService, GetPnLService, GetPortfolioService},
    domain::{portfolio::Portfolio, repository::TradeRepository},
    infrastructure::producer::KafkaTradeProducer,
};

/// Shared application state injected into every handler via Axum's [`axum::extract::State`] extractor.
///
/// All fields are cheaply cloneable (`Arc`-wrapped) so the router can clone
/// the state on every request without copying service internals.
#[derive(Clone)]
pub struct AppState {
    pub create_trade: Arc<CreateTradeService>,
    pub get_portfolio: Arc<GetPortfolioService>,
    pub get_pnl: Arc<GetPnLService>,
    /// Live when the database is reachable; `None` in degraded / in-memory mode.
    pub trade_repo: Option<Arc<dyn TradeRepository>>,
}

impl AppState {
    /// Construct state from a shared portfolio and an optional trade repository.
    ///
    /// All three services share the same `Arc<RwLock<Portfolio>>` so that
    /// trades written via `CreateTradeService` are immediately visible to
    /// `GetPortfolioService` and `GetPnLService`.
    pub fn new(
        portfolio: Arc<RwLock<Portfolio>>,
        trade_repo: Option<Arc<dyn TradeRepository>>,
        event_producer: Option<Arc<KafkaTradeProducer>>,
    ) -> Self {
        Self {
            create_trade: Arc::new(CreateTradeService::new(
                Arc::clone(&portfolio),
                trade_repo.clone(),
                event_producer,
            )),
            get_portfolio: Arc::new(GetPortfolioService::new(Arc::clone(&portfolio))),
            get_pnl: Arc::new(GetPnLService::new(Arc::clone(&portfolio))),
            trade_repo,
        }
    }
}
