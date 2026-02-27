//! Actix-web middleware for automatic OpenTelemetry tracing.

use actix_http::header::HeaderMap;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage,
};
use futures::future::{ok, LocalBoxFuture, Ready};
use opentelemetry::{
    global,
    trace::{SpanKind, Status, TraceContextExt, Tracer},
    Context, KeyValue,
};
use opentelemetry_semantic_conventions::trace::{
    CLIENT_ADDRESS, HTTP_REQUEST_METHOD, HTTP_RESPONSE_STATUS_CODE, HTTP_ROUTE,
    SERVER_ADDRESS, URL_PATH, URL_SCHEME, USER_AGENT_ORIGINAL,
};
use std::rc::Rc;

use super::common::{extract_context, span_name};

/// OpenTelemetry middleware for Actix-web.
#[derive(Clone, Default)]
pub struct OtelMiddleware {
    tracer_name: Option<&'static str>,
}

impl OtelMiddleware {
    /// Create a new middleware instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a custom tracer name.
    pub fn tracer_name(mut self, name: &'static str) -> Self {
        self.tracer_name = Some(name);
        self
    }
}

impl<S, B> Transform<S, ServiceRequest> for OtelMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = OtelMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(OtelMiddlewareService {
            service: Rc::new(service),
            tracer_name: self.tracer_name.unwrap_or("rust-otel-auto"),
        })
    }
}

/// The actual middleware service.
pub struct OtelMiddlewareService<S> {
    service: Rc<S>,
    tracer_name: &'static str,
}

impl<S, B> Service<ServiceRequest> for OtelMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let tracer = global::tracer(self.tracer_name);

        // Extract trace context from headers
        let parent_context = extract_context_from_headers(req.headers());

        // Get request details
        let method = req.method().to_string();
        let path = req.path().to_string();
        let scheme = req.connection_info().scheme().to_string();
        let host = req.connection_info().host().to_string();
        let peer_addr = req.peer_addr().map(|a| a.to_string());
        let user_agent = req
            .headers()
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Get route pattern if available
        let route_pattern = req.match_pattern();

        // Create span name
        let span_name_str = span_name(&method, &path, route_pattern.as_deref());

        // Build span attributes
        let mut attributes = vec![
            KeyValue::new(HTTP_REQUEST_METHOD, method.clone()),
            KeyValue::new(URL_PATH, path.clone()),
            KeyValue::new(URL_SCHEME, scheme),
            KeyValue::new(SERVER_ADDRESS, host),
        ];

        if let Some(ref peer) = peer_addr {
            attributes.push(KeyValue::new(CLIENT_ADDRESS, peer.clone()));
        }

        if let Some(ref ua) = user_agent {
            attributes.push(KeyValue::new(USER_AGENT_ORIGINAL, ua.clone()));
        }

        if let Some(ref route) = route_pattern {
            attributes.push(KeyValue::new(HTTP_ROUTE, route.clone()));
        }

        // Create the span
        let span = tracer
            .span_builder(span_name_str)
            .with_kind(SpanKind::Server)
            .with_attributes(attributes)
            .start_with_context(&tracer, &parent_context);

        // Store context
        let context = parent_context.with_span(span);
        req.extensions_mut().insert(context.clone());

        let service = self.service.clone();

        Box::pin(async move {
            // Attach context for the duration of the request
            let _guard = context.clone().attach();

            // Call the actual service
            let result = service.call(req).await;

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
                Err(error) => {
                    span.set_status(Status::error(error.to_string()));
                }
            }

            span.end();
            result
        })
    }
}

/// Extract trace context from Actix-web headers.
fn extract_context_from_headers(headers: &HeaderMap) -> Context {
    let headers_iter = headers.iter().filter_map(|(name, value)| {
        value.to_str().ok().map(|v| (name.as_str(), v))
    });
    extract_context(headers_iter)
}

/// Extension trait for getting the current OpenTelemetry context from a request.
pub trait OtelRequestExt {
    /// Get the OpenTelemetry context from the request extensions.
    fn otel_context(&self) -> Option<Context>;
}

impl OtelRequestExt for ServiceRequest {
    fn otel_context(&self) -> Option<Context> {
        self.extensions().get::<Context>().cloned()
    }
}
