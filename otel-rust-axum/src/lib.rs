//! Auto-instrumentation for **Rust 1.74 + OpenTelemetry 0.27 + Axum 0.6**.
//!
//! Fills the gap left by `axum-tracing-opentelemetry`, which requires otel 0.29+
//! and Rust 1.75+. This crate targets exactly:
//!
//! | Dependency | Version |
//! |---|---|
//! | `opentelemetry` | 0.27 (MSRV 1.70) |
//! | `axum` | 0.6 |
//! | `reqwest` | 0.12 |
//! | Rust | 1.74 |
//!
//! # Quick start
//!
//! ```rust,no_run
//! use axum::{Router, middleware, routing::get};
//! use otel_rust_axum::{init, current_trace_id};
//! use otel_rust_axum::layer::{OtelLayer, record_matched_route};
//!
//! #[tokio::main]
//! async fn main() {
//!     let _guard = init().expect("telemetry init failed");
//!
//!     let app = Router::new()
//!         .route("/users/:id", get(get_user))
//!         .route_layer(middleware::from_fn(record_matched_route))
//!         .layer(OtelLayer::new());
//!
//!     // ...
//! }
//!
//! async fn get_user() {
//!     tracing::info!(trace_id = %current_trace_id(), "handling request");
//! }
//! ```

mod sdk;

#[cfg(feature = "axum")]
pub mod layer;

#[cfg(feature = "reqwest")]
pub mod client;

#[cfg(feature = "db")]
pub mod db;

pub use sdk::{init, TelemetryGuard};

/// Returns the OTel trace ID of the **current span** as a 32-char hex string.
///
/// Returns `"00000000000000000000000000000000"` when called outside any span.
///
/// Use for log-trace correlation — the `trace_id` field lets you jump from a log
/// line directly to the corresponding trace in your APM tool:
///
/// ```rust,ignore
/// tracing::info!(trace_id = %otel_rust_axum::current_trace_id(), "user created");
/// ```
pub fn current_trace_id() -> String {
    use opentelemetry::trace::TraceContextExt;
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let ctx = tracing::Span::current().context();
    let span_ref = ctx.span();
    let sc = span_ref.span_context();
    if sc.is_valid() {
        sc.trace_id().to_string()
    } else {
        "0".repeat(32)
    }
}
