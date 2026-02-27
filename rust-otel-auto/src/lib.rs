//! # Rust OpenTelemetry Auto-Instrumentation
//!
//! A comprehensive auto-instrumentation library for Rust applications with OpenTelemetry support.
//! Designed to work with Rust 1.74+.

#![warn(missing_docs)]
#![allow(clippy::type_complexity)]

pub mod config;
pub mod context;
pub mod error;
#[cfg(feature = "graphql")]
pub mod graphql;
pub mod middleware;
pub mod propagator;
#[cfg(feature = "reqwest")]
pub mod reqwest_ext;
pub mod sdk;
pub mod span;

// Re-export macros
pub use rust_otel_macros::*;

// Re-export commonly used OpenTelemetry types
pub use opentelemetry::{
    global,
    trace::{SpanKind, Status, TraceContextExt, Tracer},
    Context, KeyValue,
};

use crate::error::OtelError;
use crate::sdk::OtelSdkGuard;

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::config::OtelConfig;
    pub use crate::context::ContextExt;
    #[cfg(feature = "graphql")]
    pub use crate::graphql::GraphQLTracingExtension;
    #[cfg(feature = "actix-web")]
    pub use crate::middleware::actix::OtelMiddleware as ActixOtelMiddleware;
    #[cfg(feature = "axum")]
    pub use crate::middleware::axum::OtelLayer as AxumOtelLayer;
    pub use crate::propagator::W3CTraceContextPropagator;
    #[cfg(feature = "reqwest")]
    pub use crate::reqwest_ext::TracedClient;
    pub use crate::sdk::{init, init_with_config, OtelSdkGuard};
    pub use crate::span::{SpanBuilder, SpanExt};
    pub use crate::{global, trace_span, Context, KeyValue, SpanKind, Status, Tracer};
    pub use rust_otel_macros::{instrument, traced};
}

/// Initialize OpenTelemetry with auto-configuration from environment variables.
pub fn init() -> Result<OtelSdkGuard, OtelError> {
    sdk::init()
}

/// Initialize OpenTelemetry with a custom configuration.
pub fn init_with_config(config: config::OtelConfig) -> Result<OtelSdkGuard, OtelError> {
    sdk::init_with_config(config)
}

/// Create a new span with the given name in the current context.
#[macro_export]
macro_rules! trace_span {
    ($name:expr) => {{
        use $crate::global;
        use $crate::Tracer;
        let tracer = global::tracer("rust-otel-auto");
        tracer.start($name)
    }};
    ($name:expr, $($key:expr => $value:expr),+ $(,)?) => {{
        use $crate::global;
        use $crate::Tracer;
        use $crate::KeyValue;
        let tracer = global::tracer("rust-otel-auto");
        let mut span = tracer.start($name);
        $(
            span.set_attribute(KeyValue::new($key, $value));
        )+
        span
    }};
}
