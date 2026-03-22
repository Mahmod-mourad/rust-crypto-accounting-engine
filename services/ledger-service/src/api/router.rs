use std::time::Duration;

use axum::{
    error_handling::HandleErrorLayer,
    http::StatusCode,
    routing::get,
    BoxError, Json, Router,
};
use tower::ServiceBuilder;
use tower::timeout::TimeoutLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use super::{
    handlers::{pnl::get_pnl, portfolio::get_portfolio, trade::{create_trade, list_trades}},
    health::health_check,
    response::ApiResponse,
    state::AppState,
};

/// Build the full Axum router.
///
/// # Middleware stack (outermost → innermost)
/// 1. **TraceLayer** — structured request/response logging for every request.
/// 2. **TimeoutLayer** — cancels handlers that take longer than 30 s.
/// 3. **HandleErrorLayer** — converts timeout / other middleware errors into
///    the standard `ApiResponse` envelope so the format stays consistent.
///
/// # Routes
/// | Method | Path         | Handler           |
/// |--------|--------------|-------------------|
/// | GET    | `/health`    | health check      |
/// | POST   | `/trades`    | create trade      |
/// | GET    | `/portfolio` | portfolio snapshot|
/// | GET    | `/pnl`       | PnL summary       |
pub fn build_router(state: AppState) -> Router {
    let middleware = ServiceBuilder::new()
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(HandleErrorLayer::new(handle_middleware_error))
        .layer(TimeoutLayer::new(Duration::from_secs(30)));

    Router::new()
        .route("/health", get(health_check))
        .route("/trades", get(list_trades).post(create_trade))
        .route("/portfolio", get(get_portfolio))
        .route("/pnl", get(get_pnl))
        .layer(middleware)
        .with_state(state)
}

/// Convert tower middleware errors (timeout, etc.) into the standard envelope.
async fn handle_middleware_error(err: BoxError) -> (StatusCode, Json<ApiResponse<()>>) {
    let (status, message) = if err.is::<tower::timeout::error::Elapsed>() {
        (StatusCode::REQUEST_TIMEOUT, "request timed out".to_owned())
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("internal middleware error: {err}"),
        )
    };

    (status, Json(ApiResponse::err(message)))
}
