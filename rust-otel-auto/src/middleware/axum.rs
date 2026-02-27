//! Axum middleware layer for automatic OpenTelemetry tracing.

use axum::{
    body::Body,
    http::{header, Request, Response},
};
use futures::future::BoxFuture;
use opentelemetry::{
    global,
    trace::{SpanKind, Status, TraceContextExt, Tracer},
    Context, KeyValue,
};
use opentelemetry_semantic_conventions::trace::{
    CLIENT_ADDRESS, HTTP_REQUEST_METHOD, HTTP_RESPONSE_STATUS_CODE, HTTP_ROUTE,
    SERVER_ADDRESS, URL_PATH, URL_SCHEME, USER_AGENT_ORIGINAL,
};
use std::task::{Context as TaskContext, Poll};
use tower::{Layer, Service};

use super::common::extract_context;

/// OpenTelemetry layer for Axum.
#[derive(Clone, Default)]
pub struct OtelLayer {
    tracer_name: Option<&'static str>,
}

impl OtelLayer {
    /// Create a new layer instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a custom tracer name.
    pub fn tracer_name(mut self, name: &'static str) -> Self {
        self.tracer_name = Some(name);
        self
    }
}

impl<S> Layer<S> for OtelLayer {
    type Service = OtelMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        OtelMiddleware {
            inner,
            tracer_name: self.tracer_name.unwrap_or("rust-otel-auto"),
        }
    }
}

/// The middleware service.
#[derive(Clone)]
pub struct OtelMiddleware<S> {
    inner: S,
    tracer_name: &'static str,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for OtelMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let tracer = global::tracer(self.tracer_name);
        let mut inner = self.inner.clone();

        // Extract trace context from headers
        let parent_context = extract_context_from_request(&req);

        // Get request details
        let method = req.method().to_string();
        let uri = req.uri().clone();
        let path = uri.path().to_string();
        let scheme = uri.scheme_str().unwrap_or("http").to_string();
        let host = uri.host().unwrap_or("unknown").to_string();
        let http_version = format!("{:?}", req.version());

        // Get optional headers
        let user_agent = req
            .headers()
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Create span name
        let span_name = format!("{} {}", method, path);

        // Build span attributes
        let mut attributes = vec![
            KeyValue::new(HTTP_REQUEST_METHOD, method.clone()),
            KeyValue::new(URL_PATH, path.clone()),
            KeyValue::new(URL_SCHEME, scheme),
            KeyValue::new(SERVER_ADDRESS, host),
        ];

        if let Some(ref ua) = user_agent {
            attributes.push(KeyValue::new(USER_AGENT_ORIGINAL, ua.clone()));
        }

        // Create the span
        let span = tracer
            .span_builder(span_name)
            .with_kind(SpanKind::Server)
            .with_attributes(attributes)
            .start_with_context(&tracer, &parent_context);

        // Store context
        let context = parent_context.with_span(span);

        Box::pin(async move {
            // We need to use a scope-based approach for Send compatibility
            let result = {
                let _guard = context.clone().attach();
                inner.call(req).await
            };

            // Get span from context and record response
            let span = context.span();

            match &result {
                Ok(response) => {
                    let status_code = response.status().as_u16();
                    span.set_attribute(KeyValue::new(
                        HTTP_RESPONSE_STATUS_CODE,
                        status_code as i64,
                    ));

                    if status_code >= 500 {
                        span.set_status(Status::error(format!("HTTP {}", status_code)));
                    } else {
                        span.set_status(Status::Ok);
                    }
                }
                Err(_) => {
                    span.set_status(Status::error("Service error"));
                }
            }

            span.end();
            result
        })
    }
}

/// Extract trace context from request headers.
fn extract_context_from_request<B>(req: &Request<B>) -> Context {
    let headers_iter = req.headers().iter().filter_map(|(name, value)| {
        value.to_str().ok().map(|v| (name.as_str(), v))
    });
    extract_context(headers_iter)
}
