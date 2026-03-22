use axum::{
    async_trait,
    body::Body,
    extract::{FromRequest, Request},
    Json,
};
use serde::de::DeserializeOwned;

use super::error::ApiError;

/// A JSON body extractor that converts Axum's opaque `JsonRejection` into a
/// typed [`ApiError`] (400 Bad Request) with a human-readable message.
///
/// Use this in place of bare `Json<T>` in handler signatures to get consistent
/// error envelopes on malformed payloads:
///
/// ```text
/// { "data": null, "error": "Failed to parse the request body as JSON: ..." }
/// ```
pub struct ValidatedJson<T>(pub T);

#[async_trait]
impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request<Body>, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(req, state)
            .await
            .map(|Json(value)| ValidatedJson(value))
            .map_err(|rejection| ApiError::bad_request(rejection.body_text()))
    }
}
