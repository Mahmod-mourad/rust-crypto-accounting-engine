use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use metrics::counter;
use rust_decimal::Decimal;

use crate::{
    api::{error::ApiError, error::ApiResult, extractor::ValidatedJson, response::ApiResponse, state::AppState},
    application::dto::trade::{TradeSideRequest, TradeRequest, TradeResponse},
    domain::trade::TradeSide,
};

/// `POST /trades`
///
/// Creates a validated trade and applies it to the shared in-memory portfolio.
///
/// # Request body
/// ```json
/// { "asset": "BTC", "quantity": "1.5", "price": "45000", "side": "buy" }
/// ```
///
/// # Responses
/// | Status | Meaning                                     |
/// |--------|---------------------------------------------|
/// | 201    | Trade accepted; returns `TradeResponse`     |
/// | 400    | Malformed JSON                              |
/// | 422    | Business-rule violation (e.g. no position)  |
#[tracing::instrument(skip(state), fields(asset = %body.asset))]
pub async fn create_trade(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<TradeRequest>,
) -> ApiResult<impl IntoResponse> {
    let asset = body.asset.clone();
    let side: &'static str = match body.side {
        TradeSideRequest::Buy  => "buy",
        TradeSideRequest::Sell => "sell",
    };

    let resp = state
        .create_trade
        .execute(body)
        .await
        .map_err(|e| {
            counter!("ledger_trades_total", "asset" => asset.clone(), "side" => side, "status" => "error").increment(1);
            ApiError::from(e)
        })?;

    counter!("ledger_trades_total", "asset" => asset, "side" => side, "status" => "ok").increment(1);

    Ok((StatusCode::CREATED, Json(ApiResponse::ok(resp))))
}

/// `GET /trades`
///
/// Returns all persisted trades, newest first.
/// Responds with 503 when the database is not connected.
///
/// # Responses
/// | Status | Meaning                              |
/// |--------|--------------------------------------|
/// | 200    | Array of `TradeResponse` objects     |
/// | 503    | Database not available               |
pub async fn list_trades(
    State(state): State<AppState>,
) -> ApiResult<impl IntoResponse> {
    let repo = state.trade_repo.as_ref().ok_or_else(|| {
        ApiError::from(anyhow::anyhow!("persistence not available: database is not connected"))
    })?;

    let trades = repo
        .get_trades()
        .await
        .map_err(|e| ApiError::from(anyhow::anyhow!(e)))?;

    let items: Vec<TradeResponse> = trades
        .into_iter()
        .map(|t| {
            let side = match t.side {
                TradeSide::Buy => "buy",
                TradeSide::Sell => "sell",
            };
            let notional_value = t.notional_value();
            TradeResponse {
                id: t.id,
                asset: t.asset,
                quantity: t.quantity,
                price: t.price,
                side: side.to_owned(),
                notional_value,
                timestamp: t.timestamp,
                // Realized PnL is not stored per-trade; use the portfolio endpoint for PnL.
                realized_pnl: Decimal::ZERO,
            }
        })
        .collect();

    Ok((StatusCode::OK, Json(ApiResponse::ok(items))))
}
