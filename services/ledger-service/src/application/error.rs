/// Application-level error type wrapping domain errors and infrastructure failures.
/// Uses `anyhow` for flexible error propagation across use-case boundaries.
///
/// `anyhow::Error` already provides a blanket `From<E: std::error::Error>` impl,
/// so `DomainError` (which derives `thiserror::Error`) converts automatically via `?`.
pub type AppError = anyhow::Error;
pub type AppResult<T> = anyhow::Result<T>;
