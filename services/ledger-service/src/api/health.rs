use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

/// GET /health
///
/// Returns 200 OK when the service is alive. Used by load balancers
/// and orchestrators to determine readiness.
pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}
