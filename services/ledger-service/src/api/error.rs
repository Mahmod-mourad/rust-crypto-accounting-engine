use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use crate::domain::{error::DomainError, errors::TradeError};

use super::response::ApiResponse;

/// Unified API error type that maps domain / application errors to HTTP responses.
///
/// All handler return types use `ApiResult<T>` which resolves to `Result<T, ApiError>`.
/// Axum calls `into_response()` automatically when a handler returns `Err(ApiError)`.
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }

    pub fn unprocessable(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
}

/// Serialise as `{ "data": null, "error": "..." }` with the appropriate status code.
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(ApiResponse::<()>::err(self.message))).into_response()
    }
}

/// Map ledger domain errors → HTTP status codes.
impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::AccountNotFound(_) | DomainError::TransactionNotFound(_) => {
                ApiError::not_found(err.to_string())
            }
            DomainError::InsufficientBalance { .. }
            | DomainError::InvalidCurrencyPair { .. }
            | DomainError::DuplicateTransaction(_) => ApiError::unprocessable(err.to_string()),
        }
    }
}

/// Map application-layer `anyhow::Error` → HTTP status codes.
///
/// Attempts to downcast to a typed [`TradeError`] first so that domain
/// validation failures (e.g. insufficient balance) get 422 rather than 500.
/// Falls back to 422 for all other errors (e.g. bare `anyhow::bail!` strings).
impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        if let Some(trade_err) = err.downcast_ref::<TradeError>() {
            return match trade_err {
                TradeError::InsufficientBalance { .. } | TradeError::InvalidTrade(_) => {
                    ApiError::unprocessable(trade_err.to_string())
                }
                TradeError::Overflow => ApiError::internal("arithmetic overflow"),
                TradeError::Persistence(msg) => ApiError::internal(msg.as_str()),
            };
        }
        // Bare bail!/context errors are validation messages — surface them as 422.
        ApiError::unprocessable(format!("{err:#}"))
    }
}

/// Convenience alias used in every handler return type.
pub type ApiResult<T> = Result<T, ApiError>;
