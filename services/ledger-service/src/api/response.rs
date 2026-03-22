use serde::Serialize;

/// Canonical response envelope for every API endpoint.
///
/// Success:  `{ "data": { ... }, "error": null }`
/// Failure:  `{ "data": null,    "error": "human-readable message" }`
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    /// Wrap a successful payload.
    pub fn ok(data: T) -> Self {
        Self {
            data: Some(data),
            error: None,
        }
    }
}

impl ApiResponse<()> {
    /// Construct an error envelope (no data).
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            data: None,
            error: Some(message.into()),
        }
    }
}
