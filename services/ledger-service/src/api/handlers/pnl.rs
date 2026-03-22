use axum::{extract::State, response::IntoResponse, Json};

use crate::{
    api::{error::ApiResult, extractor::ValidatedJson, response::ApiResponse, state::AppState},
    application::dto::pnl::PnLRequest,
};

/// `GET /pnl`
///
/// Computes a full PnL summary — realised (from closed lots) plus
/// unrealised (mark-to-market at caller-supplied prices).
///
/// The caller must include the current market price for **every** asset
/// that has an open position; the request is rejected otherwise.
///
/// # Request body
/// ```json
/// { "prices": { "BTC": "45000", "ETH": "2500" } }
/// ```
///
/// # Responses
/// | Status | Meaning                                           |
/// |--------|---------------------------------------------------|
/// | 200    | Returns `PnLResponse`                             |
/// | 400    | Malformed JSON                                    |
/// | 422    | A price is missing for one or more open positions |
pub async fn get_pnl(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<PnLRequest>,
) -> ApiResult<impl IntoResponse> {
    let resp = state
        .get_pnl
        .execute(body)
        .map_err(crate::api::error::ApiError::from)?;

    Ok(Json(ApiResponse::ok(resp)))
}
