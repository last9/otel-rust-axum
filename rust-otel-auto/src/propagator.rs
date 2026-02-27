//! W3C Trace Context propagator implementation.

use opentelemetry::{
    propagation::{Extractor, Injector, TextMapPropagator},
    trace::{SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState},
    Context,
};

/// W3C Trace Context header name for the main trace context.
pub const TRACEPARENT_HEADER: &str = "traceparent";

/// W3C Trace Context header name for vendor-specific trace state.
pub const TRACESTATE_HEADER: &str = "tracestate";

/// W3C Trace Context propagator.
#[derive(Debug, Default)]
pub struct W3CTraceContextPropagator;

impl W3CTraceContextPropagator {
    /// Create a new W3C Trace Context propagator.
    pub fn new() -> Self {
        Self
    }

    /// Parse a traceparent header value.
    pub fn parse_traceparent(value: &str) -> Option<SpanContext> {
        let parts: Vec<&str> = value.trim().split('-').collect();

        if parts.len() != 4 {
            return None;
        }

        let version = parts[0];
        let trace_id_hex = parts[1];
        let span_id_hex = parts[2];
        let flags_hex = parts[3];

        // Validate version
        if version.len() != 2 {
            return None;
        }

        // Validate and parse trace ID (32 hex chars = 16 bytes)
        if trace_id_hex.len() != 32 {
            return None;
        }
        let trace_id = TraceId::from_hex(trace_id_hex).ok()?;

        if trace_id == TraceId::INVALID {
            return None;
        }

        // Validate and parse span ID (16 hex chars = 8 bytes)
        if span_id_hex.len() != 16 {
            return None;
        }
        let span_id = SpanId::from_hex(span_id_hex).ok()?;

        if span_id == SpanId::INVALID {
            return None;
        }

        // Parse flags (2 hex chars = 1 byte)
        if flags_hex.len() != 2 {
            return None;
        }
        let flags_byte = u8::from_str_radix(flags_hex, 16).ok()?;
        let trace_flags = TraceFlags::new(flags_byte);

        Some(SpanContext::new(
            trace_id,
            span_id,
            trace_flags,
            true,
            TraceState::default(),
        ))
    }

    /// Format a traceparent header value.
    pub fn format_traceparent(span_context: &SpanContext) -> String {
        format!(
            "00-{}-{}-{:02x}",
            span_context.trace_id(),
            span_context.span_id(),
            span_context.trace_flags().to_u8()
        )
    }
}

impl TextMapPropagator for W3CTraceContextPropagator {
    fn inject_context(&self, cx: &Context, injector: &mut dyn Injector) {
        let span = cx.span();
        let span_context = span.span_context();

        if span_context.is_valid() {
            let traceparent = Self::format_traceparent(span_context);
            injector.set(TRACEPARENT_HEADER, traceparent);
        }
    }

    fn extract_with_context(&self, cx: &Context, extractor: &dyn Extractor) -> Context {
        let traceparent = extractor.get(TRACEPARENT_HEADER);

        if let Some(traceparent_value) = traceparent {
            if let Some(span_context) = Self::parse_traceparent(traceparent_value) {
                return cx.with_remote_span_context(span_context);
            }
        }

        cx.clone()
    }

    fn fields(&self) -> opentelemetry::propagation::text_map_propagator::FieldIter<'_> {
        opentelemetry::propagation::text_map_propagator::FieldIter::new(&[
            TRACEPARENT_HEADER,
            TRACESTATE_HEADER,
        ])
    }
}

/// Check if a trace is sampled based on the traceparent header.
pub fn is_sampled(traceparent: &str) -> bool {
    if let Some(span_context) = W3CTraceContextPropagator::parse_traceparent(traceparent) {
        span_context.trace_flags().is_sampled()
    } else {
        false
    }
}

/// Generate a new trace ID.
pub fn generate_trace_id() -> TraceId {
    use opentelemetry_sdk::trace::RandomIdGenerator;
    use opentelemetry_sdk::trace::IdGenerator;

    RandomIdGenerator::default().new_trace_id()
}

/// Generate a new span ID.
pub fn generate_span_id() -> SpanId {
    use opentelemetry_sdk::trace::RandomIdGenerator;
    use opentelemetry_sdk::trace::IdGenerator;

    RandomIdGenerator::default().new_span_id()
}

/// Create a traceparent header for a new root span.
pub fn create_root_traceparent(sampled: bool) -> String {
    let trace_id = generate_trace_id();
    let span_id = generate_span_id();
    let flags = if sampled { "01" } else { "00" };

    format!("00-{}-{}-{}", trace_id, span_id, flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_traceparent() {
        let traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let span_context = W3CTraceContextPropagator::parse_traceparent(traceparent);
        assert!(span_context.is_some());
    }

    #[test]
    fn test_is_sampled() {
        assert!(is_sampled("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"));
        assert!(!is_sampled("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00"));
    }
}
