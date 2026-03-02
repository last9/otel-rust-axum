use opentelemetry::{global, trace::TracerProvider as _, KeyValue};
use opentelemetry_otlp::{SpanExporter, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{
    propagation::TraceContextPropagator,
    runtime::Tokio,
    trace::{Sampler, TracerProvider},
    Resource,
};
use std::{collections::HashMap, env, time::Duration};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Returned by [`init`]. Shuts down the OTel SDK and flushes pending spans on drop.
///
/// Keep this alive for the duration of your program:
/// ```rust,no_run
/// let _guard = otel_rust_axum::init().unwrap();
/// ```
#[must_use = "drop this only when the program is shutting down"]
pub struct TelemetryGuard;

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        global::shutdown_tracer_provider();
    }
}

/// Initialize OpenTelemetry from standard `OTEL_*` environment variables.
///
/// | Variable | Default |
/// |---|---|
/// | `OTEL_SERVICE_NAME` | `"service"` |
/// | `OTEL_SERVICE_VERSION` | `"0.1.0"` |
/// | `OTEL_EXPORTER_OTLP_ENDPOINT` | `"http://localhost:4318"` |
/// | `OTEL_EXPORTER_OTLP_HEADERS` | _(none)_ — comma-separated `key=value` pairs |
/// | `DEPLOYMENT_ENVIRONMENT` | `"production"` |
/// | `RUST_LOG` | `"info"` |
///
/// Installs a JSON-format `tracing_subscriber` so every `tracing::info!(...)` call
/// emits a structured log line including any `trace_id` fields you record.
pub fn init() -> Result<TelemetryGuard, Box<dyn std::error::Error>> {
    let endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4318".to_string());

    let service_name = env::var("OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| "service".to_string());

    let service_version = env::var("OTEL_SERVICE_VERSION")
        .unwrap_or_else(|_| "0.1.0".to_string());

    let deployment_env = env::var("DEPLOYMENT_ENVIRONMENT")
        .unwrap_or_else(|_| "production".to_string());

    // Parse "Authorization=Basic xyz,X-Tenant=abc" into a header map
    let headers: HashMap<String, String> = env::var("OTEL_EXPORTER_OTLP_HEADERS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some(k), Some(v)) if !k.trim().is_empty() => {
                    Some((k.trim().to_string(), v.trim().to_string()))
                }
                _ => None,
            }
        })
        .collect();

    let traces_endpoint = if endpoint.ends_with("/v1/traces") {
        endpoint.clone()
    } else {
        format!("{}/v1/traces", endpoint.trim_end_matches('/'))
    };

    // otel 0.27 builder API (new_exporter() was removed)
    let exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(&traces_endpoint)
        .with_timeout(Duration::from_secs(10))
        .with_headers(headers)
        .build()?;

    let resource = Resource::new(vec![
        KeyValue::new("service.name", service_name.clone()),
        KeyValue::new("service.version", service_version),
        KeyValue::new("deployment.environment", deployment_env),
    ]);

    // otel 0.27 TracerProvider builder API (.with_config() is deprecated)
    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, Tokio)
        .with_resource(resource)
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::AlwaysOn)))
        .build();

    global::set_tracer_provider(provider.clone());

    // W3C TraceContext propagator — enables traceparent header injection/extraction
    global::set_text_map_propagator(TraceContextPropagator::new());

    let tracer = provider.tracer(service_name);
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(otel_layer)
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    Ok(TelemetryGuard)
}
