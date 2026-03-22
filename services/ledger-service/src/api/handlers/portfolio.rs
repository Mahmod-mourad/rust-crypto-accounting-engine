use axum::{extract::State, response::IntoResponse, Json};

use crate::api::{error::ApiResult, response::ApiResponse, state::AppState};

/// `GET /portfolio`
///
/// Returns a snapshot of the current portfolio: all open positions
/// (non-zero quantity) sorted alphabetically, plus cumulative realised PnL.
///
/// # Responses
/// | Status | Meaning                    |
/// |--------|----------------------------|
/// | 200    | Returns `PortfolioResponse` |
pub async fn get_portfolio(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let resp = state
        .get_portfolio
        .execute()
        .map_err(crate::api::error::ApiError::from)?;

    Ok(Json(ApiResponse::ok(resp)))
}
