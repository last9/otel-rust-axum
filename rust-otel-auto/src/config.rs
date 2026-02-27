//! Configuration module for OpenTelemetry SDK setup.
//!
//! Provides environment variable-based configuration following the
//! OpenTelemetry specification and conventions.

use std::env;
use std::time::Duration;

/// Sampling strategy for traces.
#[derive(Debug, Clone, PartialEq)]
pub enum Sampler {
    /// Always sample all traces
    AlwaysOn,
    /// Never sample any traces
    AlwaysOff,
    /// Sample based on trace ID ratio
    TraceIdRatio(f64),
    /// Honor parent's sampling decision, default to always on
    ParentBasedAlwaysOn,
    /// Honor parent's sampling decision, default to always off
    ParentBasedAlwaysOff,
    /// Honor parent's sampling decision, default to trace ID ratio
    ParentBasedTraceIdRatio(f64),
}

impl Default for Sampler {
    fn default() -> Self {
        Sampler::ParentBasedAlwaysOn
    }
}

impl Sampler {
    /// Parse sampler from environment variable string
    pub fn from_str(s: &str, arg: Option<f64>) -> Self {
        match s.to_lowercase().as_str() {
            "always_on" => Sampler::AlwaysOn,
            "always_off" => Sampler::AlwaysOff,
            "traceid_ratio" => Sampler::TraceIdRatio(arg.unwrap_or(1.0)),
            "parentbased_always_on" => Sampler::ParentBasedAlwaysOn,
            "parentbased_always_off" => Sampler::ParentBasedAlwaysOff,
            "parentbased_traceid_ratio" => Sampler::ParentBasedTraceIdRatio(arg.unwrap_or(1.0)),
            _ => Sampler::default(),
        }
    }
}

/// Configuration for the OpenTelemetry SDK.
#[derive(Debug, Clone)]
pub struct OtelConfig {
    /// Service name for traces
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Deployment environment
    pub deployment_environment: String,
    /// OTLP exporter endpoint
    pub endpoint: String,
    /// Headers for OTLP exporter (key=value pairs)
    pub headers: Vec<(String, String)>,
    /// Sampling strategy
    pub sampler: Sampler,
    /// Maximum attribute value length
    pub attribute_value_length_limit: usize,
    /// Export timeout
    pub export_timeout: Duration,
    /// Batch span processor configuration
    pub batch_config: BatchConfig,
    /// Enable console exporter for debugging
    pub console_exporter: bool,
    /// Additional resource attributes
    pub resource_attributes: Vec<(String, String)>,
}

/// Configuration for the batch span processor.
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum queue size
    pub max_queue_size: usize,
    /// Maximum export batch size
    pub max_export_batch_size: usize,
    /// Scheduled delay between exports
    pub scheduled_delay: Duration,
    /// Maximum allowed export duration
    pub max_export_timeout: Duration,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 2048,
            max_export_batch_size: 512,
            scheduled_delay: Duration::from_millis(5000),
            max_export_timeout: Duration::from_secs(30),
        }
    }
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            service_name: "unknown-service".to_string(),
            service_version: "1.0.0".to_string(),
            deployment_environment: "production".to_string(),
            endpoint: "http://localhost:4318".to_string(),
            headers: Vec::new(),
            sampler: Sampler::default(),
            attribute_value_length_limit: 256,
            export_timeout: Duration::from_secs(10),
            batch_config: BatchConfig::default(),
            console_exporter: false,
            resource_attributes: Vec::new(),
        }
    }
}

impl OtelConfig {
    /// Create a new configuration builder.
    pub fn builder() -> OtelConfigBuilder {
        OtelConfigBuilder::default()
    }

    /// Load configuration from environment variables.
    ///
    /// Supported environment variables:
    /// - `OTEL_SERVICE_NAME`: Service name
    /// - `OTEL_SERVICE_VERSION`: Service version
    /// - `OTEL_EXPORTER_OTLP_ENDPOINT`: OTLP endpoint URL
    /// - `OTEL_EXPORTER_OTLP_HEADERS`: Comma-separated key=value pairs
    /// - `OTEL_TRACES_SAMPLER`: Sampling strategy
    /// - `OTEL_TRACES_SAMPLER_ARG`: Sampler argument (e.g., ratio)
    /// - `OTEL_ATTRIBUTE_VALUE_LENGTH_LIMIT`: Max attribute value length
    /// - `DEPLOYMENT_ENVIRONMENT`: Deployment environment
    /// - `OTEL_RESOURCE_ATTRIBUTES`: Additional resource attributes
    /// - `OTEL_EXPORTER_CONSOLE`: Enable console exporter ("true"/"1")
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(name) = env::var("OTEL_SERVICE_NAME") {
            config.service_name = name;
        }

        if let Ok(version) = env::var("OTEL_SERVICE_VERSION") {
            config.service_version = version;
        }

        if let Ok(env_name) = env::var("DEPLOYMENT_ENVIRONMENT") {
            config.deployment_environment = env_name;
        }

        if let Ok(endpoint) = env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            config.endpoint = endpoint;
        }

        if let Ok(headers_str) = env::var("OTEL_EXPORTER_OTLP_HEADERS") {
            config.headers = parse_headers(&headers_str);
        }

        let sampler_arg = env::var("OTEL_TRACES_SAMPLER_ARG")
            .ok()
            .and_then(|s| s.parse::<f64>().ok());

        if let Ok(sampler_str) = env::var("OTEL_TRACES_SAMPLER") {
            config.sampler = Sampler::from_str(&sampler_str, sampler_arg);
        }

        if let Ok(limit_str) = env::var("OTEL_ATTRIBUTE_VALUE_LENGTH_LIMIT") {
            if let Ok(limit) = limit_str.parse::<usize>() {
                config.attribute_value_length_limit = limit;
            }
        }

        if let Ok(attrs) = env::var("OTEL_RESOURCE_ATTRIBUTES") {
            config.resource_attributes = parse_attributes(&attrs);
        }

        if let Ok(console) = env::var("OTEL_EXPORTER_CONSOLE") {
            config.console_exporter = console == "true" || console == "1";
        }

        config
    }
}

/// Builder for OtelConfig
#[derive(Default)]
pub struct OtelConfigBuilder {
    config: OtelConfig,
}

impl OtelConfigBuilder {
    /// Set the service name.
    pub fn service_name(mut self, name: impl Into<String>) -> Self {
        self.config.service_name = name.into();
        self
    }

    /// Set the service version.
    pub fn service_version(mut self, version: impl Into<String>) -> Self {
        self.config.service_version = version.into();
        self
    }

    /// Set the deployment environment.
    pub fn deployment_environment(mut self, env: impl Into<String>) -> Self {
        self.config.deployment_environment = env.into();
        self
    }

    /// Set the OTLP endpoint.
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.config.endpoint = endpoint.into();
        self
    }

    /// Add a header for the OTLP exporter.
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.headers.push((key.into(), value.into()));
        self
    }

    /// Set the sampling strategy.
    pub fn sampler(mut self, sampler: Sampler) -> Self {
        self.config.sampler = sampler;
        self
    }

    /// Set the attribute value length limit.
    pub fn attribute_value_length_limit(mut self, limit: usize) -> Self {
        self.config.attribute_value_length_limit = limit;
        self
    }

    /// Set the export timeout.
    pub fn export_timeout(mut self, timeout: Duration) -> Self {
        self.config.export_timeout = timeout;
        self
    }

    /// Set batch processor configuration.
    pub fn batch_config(mut self, batch_config: BatchConfig) -> Self {
        self.config.batch_config = batch_config;
        self
    }

    /// Enable console exporter for debugging.
    pub fn console_exporter(mut self, enabled: bool) -> Self {
        self.config.console_exporter = enabled;
        self
    }

    /// Add a resource attribute.
    pub fn resource_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.resource_attributes.push((key.into(), value.into()));
        self
    }

    /// Build the configuration.
    pub fn build(self) -> OtelConfig {
        self.config
    }
}

/// Parse headers from a comma-separated string of key=value pairs.
fn parse_headers(s: &str) -> Vec<(String, String)> {
    s.split(',')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some(key), Some(value)) => {
                    // URL-decode the value (basic implementation)
                    let decoded_value = value.replace('+', " ").replace("%20", " ");
                    Some((key.trim().to_string(), decoded_value.trim().to_string()))
                }
                _ => None,
            }
        })
        .collect()
}

/// Parse resource attributes from a comma-separated string.
fn parse_attributes(s: &str) -> Vec<(String, String)> {
    s.split(',')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some(key), Some(value)) => {
                    Some((key.trim().to_string(), value.trim().to_string()))
                }
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OtelConfig::default();
        assert_eq!(config.service_name, "unknown-service");
        assert_eq!(config.endpoint, "http://localhost:4318");
        assert!(matches!(config.sampler, Sampler::ParentBasedAlwaysOn));
    }

    #[test]
    fn test_sampler_parsing() {
        assert!(matches!(
            Sampler::from_str("always_on", None),
            Sampler::AlwaysOn
        ));
        assert!(matches!(
            Sampler::from_str("always_off", None),
            Sampler::AlwaysOff
        ));
        assert!(matches!(
            Sampler::from_str("traceid_ratio", Some(0.5)),
            Sampler::TraceIdRatio(r) if (r - 0.5).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn test_parse_headers() {
        let headers = parse_headers("Authorization=Bearer+token,X-Custom=value");
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0], ("Authorization".to_string(), "Bearer token".to_string()));
        assert_eq!(headers[1], ("X-Custom".to_string(), "value".to_string()));
    }

    #[test]
    fn test_builder() {
        let config = OtelConfig::builder()
            .service_name("my-service")
            .endpoint("http://collector:4318")
            .sampler(Sampler::AlwaysOn)
            .build();

        assert_eq!(config.service_name, "my-service");
        assert_eq!(config.endpoint, "http://collector:4318");
        assert!(matches!(config.sampler, Sampler::AlwaysOn));
    }
}
