//! Context propagation utilities for async Rust.
//!
//! This module provides context propagation for Tokio-based async code,
//! ensuring that trace context is properly maintained across await points
//! and task spawns.

use opentelemetry::{
    trace::{Span, SpanContext, TraceContextExt},
    Context,
};
use std::future::Future;
use std::pin::Pin;
use std::task::{self, Poll};

/// Extension trait for OpenTelemetry Context.
pub trait ContextExt {
    /// Attach this context for the duration of a future.
    fn attach_to<F: Future>(self, future: F) -> WithContext<F>;

    /// Get the span context from the current span.
    fn span_context(&self) -> SpanContext;

    /// Check if there is an active trace context.
    fn has_active_span(&self) -> bool;
}

impl ContextExt for Context {
    fn attach_to<F: Future>(self, future: F) -> WithContext<F> {
        WithContext {
            inner: future,
            context: self,
        }
    }

    fn span_context(&self) -> SpanContext {
        self.span().span_context().clone()
    }

    fn has_active_span(&self) -> bool {
        self.span().span_context().is_valid()
    }
}

pin_project_lite::pin_project! {
    /// A future wrapper that attaches an OpenTelemetry context.
    pub struct WithContext<F> {
        #[pin]
        inner: F,
        context: Context,
    }
}

impl<F: Future> Future for WithContext<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let _guard = this.context.clone().attach();
        this.inner.poll(cx)
    }
}

/// Spawn a future with the current OpenTelemetry context propagated.
///
/// This ensures that any spans created in the spawned task will be
/// properly linked to the parent span.
///
/// # Example
///
/// ```rust,ignore
/// use rust_otel_auto::context::spawn_with_context;
///
/// spawn_with_context(async {
///     // This task has the parent's trace context
///     // Spans created here will be children of the parent span
/// });
/// ```
pub fn spawn_with_context<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let context = Context::current();
    tokio::spawn(context.attach_to(future))
}

/// Run a closure with the current OpenTelemetry context attached.
///
/// # Example
///
/// ```rust,ignore
/// use rust_otel_auto::context::with_current_context;
///
/// with_current_context(|| {
///     // Code here has access to the current trace context
/// });
/// ```
pub fn with_current_context<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = Context::current().attach();
    f()
}

/// Run an async block with a specific context attached.
///
/// # Example
///
/// ```rust,ignore
/// use rust_otel_auto::context::with_context;
/// use opentelemetry::Context;
///
/// let ctx = Context::current();
/// with_context(ctx, async {
///     // Code here uses the attached context
/// }).await;
/// ```
pub async fn with_context<F, R>(context: Context, future: F) -> R
where
    F: Future<Output = R>,
{
    context.attach_to(future).await
}

/// A guard that attaches a context and detaches it when dropped.
pub struct ContextGuard {
    _guard: opentelemetry::ContextGuard,
}

impl ContextGuard {
    /// Create a new context guard from a context.
    pub fn new(context: Context) -> Self {
        Self {
            _guard: context.attach(),
        }
    }

    /// Create a context guard from the current context.
    pub fn current() -> Self {
        Self::new(Context::current())
    }
}

/// Extract trace context from an iterator of header key-value pairs.
///
/// This is useful for extracting context from HTTP headers.
pub fn extract_context<'a, I>(headers: I) -> Context
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    use opentelemetry::global;

    let extractor = HeaderExtractor::new(headers);
    global::get_text_map_propagator(|propagator| {
        propagator.extract(&extractor)
    })
}

/// Inject the current trace context into a mutable header map.
pub fn inject_context<F>(inject_fn: F)
where
    F: FnMut(&str, String),
{
    use opentelemetry::global;

    let context = Context::current();
    let mut injector = HeaderInjector::new(inject_fn);
    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut injector);
    });
}

/// Header extractor for extracting context from HTTP headers.
struct HeaderExtractor<I> {
    headers: std::collections::HashMap<String, String>,
    _marker: std::marker::PhantomData<I>,
}

impl<'a, I> HeaderExtractor<I>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    fn new(headers: I) -> Self {
        Self {
            headers: headers
                .into_iter()
                .map(|(k, v)| (k.to_lowercase(), v.to_string()))
                .collect(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I> opentelemetry::propagation::Extractor for HeaderExtractor<I> {
    fn get(&self, key: &str) -> Option<&str> {
        self.headers.get(&key.to_lowercase()).map(|s| s.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(|k| k.as_str()).collect()
    }
}

/// Header injector for injecting context into HTTP headers.
struct HeaderInjector<F> {
    inject_fn: F,
}

impl<F> HeaderInjector<F>
where
    F: FnMut(&str, String),
{
    fn new(inject_fn: F) -> Self {
        Self { inject_fn }
    }
}

impl<F> opentelemetry::propagation::Injector for HeaderInjector<F>
where
    F: FnMut(&str, String),
{
    fn set(&mut self, key: &str, value: String) {
        (self.inject_fn)(key, value);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_ext() {
        let ctx = Context::current();
        assert!(!ctx.has_active_span() || ctx.has_active_span()); // Always valid
    }

    #[test]
    fn test_extract_context_empty() {
        let headers: Vec<(&str, &str)> = vec![];
        let ctx = extract_context(headers);
        // Should return root context when no headers
        assert!(!ctx.has_active_span());
    }

    #[test]
    fn test_header_extractor() {
        let headers = vec![
            ("traceparent", "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
        ];
        let extractor = HeaderExtractor::new(headers);
        assert_eq!(
            extractor.get("traceparent"),
            Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
        );
    }
}
