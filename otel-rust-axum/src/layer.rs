//! Axum 0.6 HTTP server instrumentation with OTel semantic conventions.
//!
//! # Usage
//!
//! ```rust,no_run
//! use axum::{Router, middleware, routing::get};
//! use otel_rust_axum::layer::{OtelLayer, record_matched_route};
//!
//! let app = Router::new()
//!     .route("/users/:id", get(handler))
//!     .route_layer(middleware::from_fn(record_matched_route))
//!     .layer(OtelLayer::new());
//! # async fn handler() {}
//! ```

use axum::{
    body::Body,
    extract::MatchedPath,
    http::{Request, Response, Version},
    middleware::Next,
    response::IntoResponse,
};
use opentelemetry::{global, propagation::Extractor, Context};
use std::time::Duration;
use tower::Layer;
use tower_http::{
    classify::{ServerErrorsAsFailures, SharedClassifier},
    trace::{
        DefaultOnBodyChunk, DefaultOnEos, DefaultOnFailure, MakeSpan, OnResponse, Trace,
        TraceLayer,
    },
};
use tracing::Span;

// ---------------------------------------------------------------------------
// Internal type aliases for the configured TraceLayer and its Service output
// ---------------------------------------------------------------------------

type OtelTraceLayer = TraceLayer<
    SharedClassifier<ServerErrorsAsFailures>,
    OtelMakeSpan,
    (),
    OtelOnResponse,
>;

// The concrete service type produced by OtelTraceLayer wrapping an inner S.
// Spelling this out explicitly lets rustc infer the body type when axum calls
// `.layer(OtelLayer::new())` — otherwise the associated-type projection chain
// is too deep for the inference engine to normalize automatically.
type OtelService<S> = Trace<
    S,
    SharedClassifier<ServerErrorsAsFailures>,
    OtelMakeSpan,
    (),
    OtelOnResponse,
    DefaultOnBodyChunk,
    DefaultOnEos,
    DefaultOnFailure,
>;

// ---------------------------------------------------------------------------
// OtelLayer — the public entry point
// ---------------------------------------------------------------------------

/// A Tower [`Layer`] that instruments every Axum request with an OTel HTTP server span.
///
/// Attributes set on each span:
/// - `http.method`, `http.target`, `http.flavor`, `http.scheme`, `http.host`
/// - `http.route` (filled in by [`record_matched_route`] middleware)
/// - `http.status_code` (filled in on response)
/// - Incoming `traceparent` / `tracestate` headers are extracted so the span
///   joins an existing distributed trace automatically.
///
/// # Ordering
///
/// Apply `route_layer` **before** `.layer`:
/// ```rust,no_run
/// use axum::{Router, middleware, routing::get};
/// use otel_rust_axum::layer::{OtelLayer, record_matched_route};
///
/// let app = Router::new()
///     .route("/users/:id", get(handler))
///     .route_layer(middleware::from_fn(record_matched_route))  // inner — has MatchedPath
///     .layer(OtelLayer::new());                                // outer — creates the span
/// # async fn handler() {}
/// ```
#[derive(Clone, Default)]
pub struct OtelLayer;

impl OtelLayer {
    pub fn new() -> Self {
        Self
    }

    fn trace_layer() -> OtelTraceLayer {
        TraceLayer::new_for_http()
            .make_span_with(OtelMakeSpan)
            .on_request(())
            .on_response(OtelOnResponse)
    }
}

impl<S> Layer<S> for OtelLayer
where
    OtelTraceLayer: Layer<S, Service = OtelService<S>>,
{
    type Service = OtelService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::trace_layer().layer(inner)
    }
}

// ---------------------------------------------------------------------------
// MakeSpan
// ---------------------------------------------------------------------------

/// Creates the HTTP server span with OTel semantic convention attributes.
#[derive(Clone)]
pub struct OtelMakeSpan;

impl<B> MakeSpan<B> for OtelMakeSpan {
    fn make_span(&mut self, req: &Request<B>) -> Span {
        let method = req.method().as_str();
        let target = req
            .uri()
            .path_and_query()
            .map(|p| p.as_str())
            .unwrap_or(req.uri().path());
        let flavor = match req.version() {
            Version::HTTP_10 => "1.0",
            Version::HTTP_11 => "1.1",
            Version::HTTP_2  => "2.0",
            Version::HTTP_3  => "3.0",
            _                => "1.1",
        };
        let scheme = req.uri().scheme_str().unwrap_or("http");
        let host = req
            .headers()
            .get("host")
            .and_then(|v| v.to_str().ok())
            .or_else(|| req.uri().host())
            .unwrap_or("localhost");

        let span = tracing::info_span!(
            "HTTP",
            "otel.kind"        = "server",
            "otel.name"        = tracing::field::Empty,
            "http.method"      = method,
            "http.target"      = target,
            "http.flavor"      = flavor,
            "http.scheme"      = scheme,
            "http.host"        = host,
            "http.route"       = tracing::field::Empty,
            "http.status_code" = tracing::field::Empty,
        );

        // Extract incoming W3C traceparent — makes this span a child of the caller's trace
        let parent_cx = extract_context(req);
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        span.set_parent(parent_cx);

        span
    }
}

// ---------------------------------------------------------------------------
// OnResponse
// ---------------------------------------------------------------------------

/// Records `http.status_code` when the response is ready.
#[derive(Clone)]
pub struct OtelOnResponse;

impl<B> OnResponse<B> for OtelOnResponse {
    fn on_response(self, response: &Response<B>, _latency: Duration, span: &Span) {
        span.record("http.status_code", response.status().as_u16());
    }
}

// ---------------------------------------------------------------------------
// record_matched_route middleware
// ---------------------------------------------------------------------------

/// Axum middleware that records `http.route` and sets the OTel span name to
/// `"{METHOD} {route}"` (e.g. `"GET /users/:id"`).
///
/// Must be applied via `route_layer` (not `.layer`) so `MatchedPath` is available.
pub async fn record_matched_route(req: Request<Body>, next: Next<Body>) -> impl IntoResponse {
    if let Some(path) = req.extensions().get::<MatchedPath>() {
        let route  = path.as_str().to_owned();
        let method = req.method().as_str().to_owned();
        let span   = Span::current();
        span.record("http.route", route.as_str());
        span.record("otel.name", format!("{} {}", method, route).as_str());
    }
    next.run(req).await
}

// ---------------------------------------------------------------------------
// W3C TraceContext extractor for incoming requests
// ---------------------------------------------------------------------------

struct HeaderExtractor<'a>(&'a axum::http::HeaderMap);

impl<'a> Extractor for HeaderExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

fn extract_context<B>(req: &Request<B>) -> Context {
    global::get_text_map_propagator(|p| p.extract(&HeaderExtractor(req.headers())))
}
