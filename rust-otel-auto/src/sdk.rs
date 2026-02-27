//! OpenTelemetry SDK initialization and management.
//!
//! This module provides the core SDK setup functionality with auto-configuration
//! from environment variables.

use crate::config::{OtelConfig, Sampler};
use crate::error::{OtelError, OtelResult};
use crate::propagator::W3CTraceContextPropagator;

use once_cell::sync::OnceCell;
use opentelemetry::global;
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::runtime::Tokio;
use opentelemetry_sdk::trace::{
    BatchSpanProcessor, RandomIdGenerator, Sampler as OtelSampler, TracerProvider as SdkTracerProvider,
};
use opentelemetry_sdk::{trace, Resource};
use opentelemetry_semantic_conventions::resource::{
    DEPLOYMENT_ENVIRONMENT, SERVICE_NAME, SERVICE_VERSION,
};
use parking_lot::Mutex;
use std::sync::Arc;

static INITIALIZED: OnceCell<()> = OnceCell::new();
static TRACER_PROVIDER: OnceCell<Arc<Mutex<Option<SdkTracerProvider>>>> = OnceCell::new();

/// Guard that ensures the OpenTelemetry SDK is properly shut down when dropped.
///
/// Keep this guard alive for the duration of your application. When dropped,
/// it will flush any pending spans and shut down the SDK gracefully.
#[must_use = "The guard must be kept alive for the SDK to function"]
pub struct OtelSdkGuard {
    _private: (),
}

impl Drop for OtelSdkGuard {
    fn drop(&mut self) {
        shutdown();
    }
}

/// Initialize the OpenTelemetry SDK with auto-configuration from environment variables.
///
/// This function will:
/// 1. Load configuration from environment variables
/// 2. Create an OTLP exporter
/// 3. Configure the tracer provider with batch processing
/// 4. Set up W3C trace context propagation
///
/// # Returns
///
/// Returns an `OtelSdkGuard` that must be kept alive for the SDK to function.
/// The SDK will be shut down when the guard is dropped.
///
/// # Errors
///
/// Returns an error if:
/// - The SDK is already initialized
/// - Failed to create the exporter
/// - Failed to create the tracer provider
pub fn init() -> OtelResult<OtelSdkGuard> {
    init_with_config(OtelConfig::from_env())
}

/// Initialize the OpenTelemetry SDK with a custom configuration.
pub fn init_with_config(config: OtelConfig) -> OtelResult<OtelSdkGuard> {
    // Check if already initialized
    if INITIALIZED.get().is_some() {
        return Err(OtelError::AlreadyInitialized);
    }

    // Set up the global text map propagator for W3C trace context
    global::set_text_map_propagator(TraceContextPropagator::new());

    // Create the resource with service information
    let resource = create_resource(&config);

    // Create the sampler
    let sampler = create_sampler(&config.sampler);

    // Create the OTLP exporter
    let exporter = create_otlp_exporter(&config)?;

    // Create the batch span processor
    let batch_processor = BatchSpanProcessor::builder(exporter, Tokio)
        .with_max_queue_size(config.batch_config.max_queue_size)
        .with_max_export_batch_size(config.batch_config.max_export_batch_size)
        .with_scheduled_delay(config.batch_config.scheduled_delay)
        .with_max_export_timeout(config.batch_config.max_export_timeout)
        .build();

    // Build the tracer provider
    let provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_sampler(sampler)
        .with_id_generator(RandomIdGenerator::default())
        .with_span_processor(batch_processor)
        .build();

    // Set as global provider
    let _ = global::set_tracer_provider(provider.clone());

    // Store the provider for shutdown
    let _ = TRACER_PROVIDER.set(Arc::new(Mutex::new(Some(provider))));

    // Mark as initialized
    let _ = INITIALIZED.set(());

    // Set up tracing-opentelemetry integration
    setup_tracing_integration();

    Ok(OtelSdkGuard { _private: () })
}

/// Create the resource with service information and custom attributes.
fn create_resource(config: &OtelConfig) -> Resource {
    let mut attributes = vec![
        opentelemetry::KeyValue::new(SERVICE_NAME, config.service_name.clone()),
        opentelemetry::KeyValue::new(SERVICE_VERSION, config.service_version.clone()),
        opentelemetry::KeyValue::new(DEPLOYMENT_ENVIRONMENT, config.deployment_environment.clone()),
    ];

    // Add custom resource attributes
    for (key, value) in &config.resource_attributes {
        attributes.push(opentelemetry::KeyValue::new(key.clone(), value.clone()));
    }

    Resource::new(attributes)
}

/// Create a sampler from the configuration.
fn create_sampler(sampler: &Sampler) -> OtelSampler {
    match sampler {
        Sampler::AlwaysOn => OtelSampler::AlwaysOn,
        Sampler::AlwaysOff => OtelSampler::AlwaysOff,
        Sampler::TraceIdRatio(ratio) => OtelSampler::TraceIdRatioBased(*ratio),
        Sampler::ParentBasedAlwaysOn => {
            OtelSampler::ParentBased(Box::new(OtelSampler::AlwaysOn))
        }
        Sampler::ParentBasedAlwaysOff => {
            OtelSampler::ParentBased(Box::new(OtelSampler::AlwaysOff))
        }
        Sampler::ParentBasedTraceIdRatio(ratio) => {
            OtelSampler::ParentBased(Box::new(OtelSampler::TraceIdRatioBased(*ratio)))
        }
    }
}

/// Create the OTLP exporter.
fn create_otlp_exporter(
    config: &OtelConfig,
) -> OtelResult<opentelemetry_otlp::SpanExporter> {
    use opentelemetry_otlp::{Protocol, WithExportConfig};

    // Ensure endpoint has /v1/traces path for HTTP exporter
    let endpoint = if config.endpoint.ends_with("/v1/traces") {
        config.endpoint.clone()
    } else {
        format!("{}/v1/traces", config.endpoint.trim_end_matches('/'))
    };

    // Build the HTTP exporter
    let mut exporter = opentelemetry_otlp::new_exporter()
        .http()
        .with_endpoint(&endpoint)
        .with_protocol(Protocol::HttpBinary)
        .with_timeout(config.export_timeout);

    // Add headers if configured (e.g., Authorization)
    if !config.headers.is_empty() {
        let headers: std::collections::HashMap<String, String> = config
            .headers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        exporter = exporter.with_headers(headers);
    }

    // Build and return the span exporter
    exporter
        .build_span_exporter()
        .map_err(|e| OtelError::ExporterCreation(e.to_string()))
}

/// Set up the tracing-opentelemetry integration.
fn setup_tracing_integration() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    let tracer = global::tracer("rust-otel-auto");
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(telemetry_layer)
        .with(tracing_subscriber::fmt::layer())
        .try_init();
}

/// Shutdown the OpenTelemetry SDK gracefully.
///
/// This function is called automatically when the `OtelSdkGuard` is dropped.
pub fn shutdown() {
    if let Some(provider) = TRACER_PROVIDER.get() {
        if let Some(provider) = provider.lock().take() {
            // Force flush and shutdown
            if let Err(e) = provider.force_flush() {
                eprintln!("Warning: Failed to flush tracer provider: {:?}", e);
            }
            if let Err(e) = provider.shutdown() {
                eprintln!("Warning: Failed to shutdown tracer provider: {:?}", e);
            }
        }
    }
    global::shutdown_tracer_provider();
}

/// Get a tracer with the specified name.
///
/// This is a convenience function for getting a tracer from the global provider.
pub fn tracer(name: &'static str) -> opentelemetry::global::BoxedTracer {
    global::tracer(name)
}

/// Check if the SDK has been initialized.
pub fn is_initialized() -> bool {
    INITIALIZED.get().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_resource() {
        let config = OtelConfig::builder()
            .service_name("test-service")
            .service_version("1.0.0")
            .resource_attribute("custom.key", "custom.value")
            .build();

        let resource = create_resource(&config);
        // Resource is created successfully
        assert!(resource.len() > 0);
    }

    #[test]
    fn test_create_sampler() {
        let always_on = create_sampler(&Sampler::AlwaysOn);
        let always_off = create_sampler(&Sampler::AlwaysOff);
        let ratio = create_sampler(&Sampler::TraceIdRatio(0.5));

        // Samplers are created (we can't easily assert their type)
        drop(always_on);
        drop(always_off);
        drop(ratio);
    }
}
