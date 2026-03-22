pub mod create_trade;
pub mod get_pnl;
pub mod get_portfolio;

// Convenience re-exports — consumed by the API layer once routes are wired up.
#[allow(unused_imports)]
pub use create_trade::CreateTradeService;
#[allow(unused_imports)]
pub use get_pnl::GetPnLService;
#[allow(unused_imports)]
pub use get_portfolio::GetPortfolioService;
