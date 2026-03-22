use std::sync::{Arc, RwLock};

use anyhow::Context;
use chrono::Utc;

use crate::{
    application::{
        dto::trade::{TradeRequest, TradeResponse, TradeSideRequest},
        error::AppResult,
    },
    domain::{
        portfolio::Portfolio,
        repository::TradeRepository,
        trade::{Trade, TradeSide},
    },
    infrastructure::producer::{KafkaTradeProducer, TradeEvent},
};

/// Creates a validated trade, applies it to the shared in-memory portfolio,
/// and (when a repository is wired up) persists it to PostgreSQL.
pub struct CreateTradeService {
    portfolio: Arc<RwLock<Portfolio>>,
    trade_repo: Option<Arc<dyn TradeRepository>>,
    event_producer: Option<Arc<KafkaTradeProducer>>,
}

impl CreateTradeService {
    pub fn new(
        portfolio: Arc<RwLock<Portfolio>>,
        trade_repo: Option<Arc<dyn TradeRepository>>,
        event_producer: Option<Arc<KafkaTradeProducer>>,
    ) -> Self {
        Self {
            portfolio,
            trade_repo,
            event_producer,
        }
    }

    /// Validate the request, apply the trade to the portfolio, persist it, and
    /// return a response DTO.
    pub async fn execute(&self, req: TradeRequest) -> AppResult<TradeResponse> {
        // ── Input validation ────────────────────────────────────────────────
        if req.asset.trim().is_empty() {
            anyhow::bail!("asset symbol must not be empty");
        }

        let side = match req.side {
            TradeSideRequest::Buy => TradeSide::Buy,
            TradeSideRequest::Sell => TradeSide::Sell,
        };
        let side_str = match side {
            TradeSide::Buy => "buy",
            TradeSide::Sell => "sell",
        };

        let timestamp = req.timestamp.unwrap_or_else(Utc::now);

        // ── Domain construction (quantity / price positivity enforced here) ─
        let trade = Trade::new(req.asset, req.quantity, req.price, timestamp, side)
            .context("invalid trade parameters")?;

        let notional_value = trade.notional_value();

        // ── Apply to in-memory portfolio (exclusive write, released before any await) ─
        let realized_pnl = {
            let mut portfolio = self.portfolio.write().expect("portfolio lock poisoned");
            let realized_events = portfolio
                .apply_trade(&trade)
                .context("failed to apply trade to portfolio")?;
            realized_events.iter().map(|e| e.realized_pnl).sum()
        };

        // ── Persist to database if a repository is available ─────────────────
        if let Some(repo) = &self.trade_repo {
            repo.save_trade(&trade)
                .await
                .context("failed to persist trade")?;
        }

        // ── Build response ───────────────────────────────────────────────────
        let response = TradeResponse {
            id: trade.id,
            asset: trade.asset,
            quantity: trade.quantity,
            price: trade.price,
            side: side_str.to_owned(),
            notional_value,
            timestamp: trade.timestamp,
            realized_pnl,
        };

        // ── Publish event (fire-and-forget — does not block the response) ────
        if let Some(producer) = self.event_producer.clone() {
            let event = TradeEvent::from_response(&response);
            tokio::spawn(async move {
                producer.publish_trade_created(&event).await;
            });
        }

        Ok(response)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;
    use crate::application::dto::trade::TradeSideRequest;

    fn portfolio() -> Arc<RwLock<Portfolio>> {
        Arc::new(RwLock::new(Portfolio::new()))
    }

    // No repository or producer needed for unit tests — both paths are optional.
    fn svc(p: Arc<RwLock<Portfolio>>) -> CreateTradeService {
        CreateTradeService::new(p, None, None)
    }

    fn buy_req(asset: &str, qty: rust_decimal::Decimal, price: rust_decimal::Decimal) -> TradeRequest {
        TradeRequest {
            asset: asset.to_owned(),
            quantity: qty,
            price,
            side: TradeSideRequest::Buy,
            timestamp: None,
        }
    }

    fn sell_req(asset: &str, qty: rust_decimal::Decimal, price: rust_decimal::Decimal) -> TradeRequest {
        TradeRequest {
            asset: asset.to_owned(),
            quantity: qty,
            price,
            side: TradeSideRequest::Sell,
            timestamp: None,
        }
    }

    #[tokio::test]
    async fn buy_trade_returns_zero_realized_pnl() {
        let resp = svc(portfolio())
            .execute(buy_req("BTC", dec!(1), dec!(30_000)))
            .await
            .unwrap();
        assert_eq!(resp.asset, "BTC");
        assert_eq!(resp.side, "buy");
        assert_eq!(resp.notional_value, dec!(30_000));
        assert_eq!(resp.realized_pnl, dec!(0));
    }

    #[tokio::test]
    async fn sell_trade_returns_realized_pnl() {
        let p = portfolio();
        let s = svc(Arc::clone(&p));

        s.execute(buy_req("ETH", dec!(10), dec!(100))).await.unwrap();
        let resp = s.execute(sell_req("ETH", dec!(5), dec!(200))).await.unwrap();

        // 5 × ($200 − $100) = $500
        assert_eq!(resp.realized_pnl, dec!(500));
        assert_eq!(resp.side, "sell");
    }

    #[tokio::test]
    async fn empty_asset_is_rejected() {
        let req = TradeRequest {
            asset: "  ".to_owned(),
            quantity: dec!(1),
            price: dec!(100),
            side: TradeSideRequest::Buy,
            timestamp: None,
        };
        assert!(svc(portfolio()).execute(req).await.is_err());
    }

    #[tokio::test]
    async fn zero_quantity_is_rejected() {
        let req = TradeRequest {
            asset: "BTC".to_owned(),
            quantity: dec!(0),
            price: dec!(100),
            side: TradeSideRequest::Buy,
            timestamp: None,
        };
        assert!(svc(portfolio()).execute(req).await.is_err());
    }

    #[tokio::test]
    async fn sell_without_position_is_rejected() {
        assert!(
            svc(portfolio())
                .execute(sell_req("BTC", dec!(1), dec!(50_000)))
                .await
                .is_err()
        );
    }
}
