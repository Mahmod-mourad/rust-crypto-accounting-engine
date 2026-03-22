//! Shared observability bootstrap for all services.
//!
//! ## What this sets up
//!
//! * **Structured JSON logs** via `tracing-subscriber` (level from `RUST_LOG`)
//! * **Distributed traces** exported over OTLP/gRPC to Jaeger (or any
//!   OpenTelemetry collector) – endpoint from `OTEL_EXPORTER_OTLP_ENDPOINT`
//!   (default `http://jaeger:4317`)
//! * **Prometheus metrics** served on `METRICS_PORT` (default `9091`) at
//!   `/metrics`, started as a background tokio task
//!
//! ## Usage
//!
//! ```rust,no_run
//! #[tokio::main]
//! async fn main() {
//!     let _guard = observability::init("my-service");
//!     // … rest of startup
//! }
//! ```
//!
//! The returned `TelemetryGuard` flushes pending spans on drop.

use opentelemetry::KeyValue;
// Import the TracerProvider trait for `.tracer()` method dispatch without
// a name conflict with the SDK struct of the same name.
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::{
    runtime::Tokio,
    trace::TracerProvider,
    Resource,
};
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, fmt};

// ── Public re-exports so callers don't need to depend on `metrics` directly ──

pub use metrics::{counter, gauge, histogram};

// ── Guard ────────────────────────────────────────────────────────────────────

/// Returned by [`init`]. Shuts down the tracer provider (flushing buffered
/// spans) when dropped at the end of `main`.
pub struct TelemetryGuard {
    provider: TracerProvider,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Err(e) = self.provider.shutdown() {
            eprintln!("opentelemetry shutdown error: {e}");
        }
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

/// Initialise tracing, distributed traces, and Prometheus metrics for
/// `service_name`.
///
/// Must be called **inside** the tokio runtime (i.e. from `#[tokio::main]`).
pub fn init(service_name: &'static str) -> TelemetryGuard {
    // ── 1. OpenTelemetry OTLP trace exporter ─────────────────────────────────
    let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://jaeger:4317".to_string());

    let provider = build_tracer_provider(service_name, &otlp_endpoint);

    // Register as the global provider so `opentelemetry::global::tracer()` works.
    opentelemetry::global::set_tracer_provider(provider.clone());
    // Call `.tracer()` on the concrete SDK type (requires the TracerProvider trait in scope).
    let tracer = provider.tracer(service_name);

    // ── 2. tracing-subscriber with JSON + EnvFilter + OTel layer ─────────────
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(fmt::layer().json())
        .with(OpenTelemetryLayer::new(tracer))
        .init();

    // ── 3. Prometheus metrics — background HTTP listener ─────────────────────
    let metrics_port: u16 = std::env::var("METRICS_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9091);

    metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(([0, 0, 0, 0], metrics_port))
        .install()
        .expect("failed to install Prometheus metrics listener");

    tracing::info!(
        otlp_endpoint,
        metrics_port,
        service = service_name,
        "observability initialised"
    );

    TelemetryGuard { provider }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn build_tracer_provider(service_name: &'static str, endpoint: &str) -> TracerProvider {
    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .expect("failed to build OTLP span exporter");

    TracerProvider::builder()
        .with_batch_exporter(exporter, Tokio)
        .with_resource(Resource::new(vec![KeyValue::new(
            SERVICE_NAME,
            service_name,
        )]))
        .build()
}
