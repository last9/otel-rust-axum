//! HTTP framework middleware for automatic tracing.
//!
//! This module provides middleware implementations for popular Rust HTTP frameworks.

#[cfg(feature = "actix-web")]
pub mod actix;

#[cfg(feature = "axum")]
pub mod axum;

/// Common utilities for extracting trace context from HTTP headers.
pub mod common {
    use opentelemetry::{global, Context};

    /// Extract trace context from HTTP headers.
    pub fn extract_context<'a, I>(headers: I) -> Context
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let extractor = HeaderExtractor::new(headers);
        global::get_text_map_propagator(|propagator| propagator.extract(&extractor))
    }

    /// Header extractor implementation.
    pub struct HeaderExtractor {
        headers: std::collections::HashMap<String, String>,
    }

    impl HeaderExtractor {
        pub fn new<'a, I>(headers: I) -> Self
        where
            I: IntoIterator<Item = (&'a str, &'a str)>,
        {
            Self {
                headers: headers
                    .into_iter()
                    .map(|(k, v)| (k.to_lowercase(), v.to_string()))
                    .collect(),
            }
        }
    }

    impl opentelemetry::propagation::Extractor for HeaderExtractor {
        fn get(&self, key: &str) -> Option<&str> {
            self.headers.get(&key.to_lowercase()).map(|s| s.as_str())
        }

        fn keys(&self) -> Vec<&str> {
            self.headers.keys().map(|k| k.as_str()).collect()
        }
    }

    /// Generate span name from HTTP method and path.
    pub fn span_name(method: &str, path: &str, route: Option<&str>) -> String {
        match route {
            Some(r) => format!("{} {}", method, r),
            None => format!("{} {}", method, path),
        }
    }

    /// Classify HTTP status code for span status.
    pub fn classify_status_code(code: u16) -> opentelemetry::trace::Status {
        if code >= 500 {
            opentelemetry::trace::Status::error(format!("HTTP {}", code))
        } else {
            opentelemetry::trace::Status::Ok
        }
    }
}
