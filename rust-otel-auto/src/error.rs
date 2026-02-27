//! Error types for the OpenTelemetry auto-instrumentation library.

use thiserror::Error;

/// Errors that can occur during OpenTelemetry initialization and operation.
#[derive(Error, Debug)]
pub enum OtelError {
    /// Failed to create the OTLP exporter
    #[error("Failed to create OTLP exporter: {0}")]
    ExporterCreation(String),

    /// Failed to create the tracer provider
    #[error("Failed to create tracer provider: {0}")]
    TracerProviderCreation(String),

    /// Failed to initialize the SDK
    #[error("Failed to initialize OpenTelemetry SDK: {0}")]
    SdkInitialization(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Context propagation error
    #[error("Context propagation error: {0}")]
    ContextPropagation(String),

    /// Span creation error
    #[error("Failed to create span: {0}")]
    SpanCreation(String),

    /// Export error
    #[error("Failed to export telemetry data: {0}")]
    Export(String),

    /// SDK already initialized
    #[error("OpenTelemetry SDK already initialized")]
    AlreadyInitialized,

    /// SDK not initialized
    #[error("OpenTelemetry SDK not initialized")]
    NotInitialized,

    /// Invalid trace context
    #[error("Invalid trace context: {0}")]
    InvalidTraceContext(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for OtelError
pub type OtelResult<T> = Result<T, OtelError>;
