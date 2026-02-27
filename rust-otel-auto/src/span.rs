//! Span utilities and builders for creating traced operations.

use opentelemetry::{
    global,
    trace::{SpanKind, Status, Tracer},
    Context, KeyValue,
};
use std::time::SystemTime;

/// Builder for creating spans with fluent API.
pub struct SpanBuilder {
    name: String,
    kind: SpanKind,
    attributes: Vec<KeyValue>,
    start_time: Option<SystemTime>,
    parent: Option<Context>,
}

impl SpanBuilder {
    /// Create a new span builder with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: SpanKind::Internal,
            attributes: Vec::new(),
            start_time: None,
            parent: None,
        }
    }

    /// Create a new server span builder.
    pub fn server(name: impl Into<String>) -> Self {
        Self::new(name).kind(SpanKind::Server)
    }

    /// Create a new client span builder.
    pub fn client(name: impl Into<String>) -> Self {
        Self::new(name).kind(SpanKind::Client)
    }

    /// Create a new producer span builder.
    pub fn producer(name: impl Into<String>) -> Self {
        Self::new(name).kind(SpanKind::Producer)
    }

    /// Create a new consumer span builder.
    pub fn consumer(name: impl Into<String>) -> Self {
        Self::new(name).kind(SpanKind::Consumer)
    }

    /// Set the span kind.
    pub fn kind(mut self, kind: SpanKind) -> Self {
        self.kind = kind;
        self
    }

    /// Add an attribute to the span.
    pub fn attribute(mut self, key: impl Into<opentelemetry::Key>, value: impl Into<opentelemetry::Value>) -> Self {
        self.attributes.push(KeyValue::new(key, value));
        self
    }

    /// Add multiple attributes to the span.
    pub fn attributes(mut self, attrs: impl IntoIterator<Item = KeyValue>) -> Self {
        self.attributes.extend(attrs);
        self
    }

    /// Set the start time of the span.
    pub fn start_time(mut self, time: SystemTime) -> Self {
        self.start_time = Some(time);
        self
    }

    /// Set the parent context for the span.
    pub fn parent(mut self, parent: Context) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Build and start the span, returning the context with the span.
    pub fn start(self) -> Context {
        let tracer = global::tracer("rust-otel-auto");
        let mut builder = tracer.span_builder(self.name);
        builder = builder.with_kind(self.kind);

        if !self.attributes.is_empty() {
            builder = builder.with_attributes(self.attributes);
        }

        if let Some(start_time) = self.start_time {
            builder = builder.with_start_time(start_time);
        }

        let parent_context = self.parent.unwrap_or_else(Context::current);
        let span = tracer.build_with_context(builder, &parent_context);

        use opentelemetry::trace::TraceContextExt;
        parent_context.with_span(span)
    }
}

/// Extension trait for spans.
pub trait SpanExt {
    /// Set an error status on the span.
    fn set_error(&self, message: impl Into<String>);

    /// Set a success status on the span.
    fn set_ok(&self);

    /// Add HTTP request attributes to the span.
    fn set_http_request_attributes(
        &self,
        method: &str,
        url: &str,
        route: Option<&str>,
    );

    /// Add HTTP response attributes to the span.
    fn set_http_response_attributes(&self, status_code: u16);

    /// Add GraphQL attributes to the span.
    fn set_graphql_attributes(
        &self,
        operation_name: Option<&str>,
        operation_type: &str,
    );
}

impl SpanExt for Context {
    fn set_error(&self, message: impl Into<String>) {
        use opentelemetry::trace::TraceContextExt;
        self.span().set_status(Status::error(message.into()));
    }

    fn set_ok(&self) {
        use opentelemetry::trace::TraceContextExt;
        self.span().set_status(Status::Ok);
    }

    fn set_http_request_attributes(
        &self,
        method: &str,
        url: &str,
        route: Option<&str>,
    ) {
        use opentelemetry::trace::TraceContextExt;
        use opentelemetry_semantic_conventions::trace::{
            HTTP_REQUEST_METHOD, HTTP_ROUTE, URL_FULL,
        };

        let span = self.span();
        span.set_attribute(KeyValue::new(HTTP_REQUEST_METHOD, method.to_string()));
        span.set_attribute(KeyValue::new(URL_FULL, url.to_string()));

        if let Some(route) = route {
            span.set_attribute(KeyValue::new(HTTP_ROUTE, route.to_string()));
        }
    }

    fn set_http_response_attributes(&self, status_code: u16) {
        use opentelemetry::trace::TraceContextExt;
        use opentelemetry_semantic_conventions::trace::HTTP_RESPONSE_STATUS_CODE;

        let span = self.span();
        span.set_attribute(KeyValue::new(
            HTTP_RESPONSE_STATUS_CODE,
            status_code as i64,
        ));

        if status_code >= 500 {
            span.set_status(Status::error(format!("HTTP {}", status_code)));
        }
    }

    fn set_graphql_attributes(
        &self,
        operation_name: Option<&str>,
        operation_type: &str,
    ) {
        use opentelemetry::trace::TraceContextExt;

        let span = self.span();
        if let Some(name) = operation_name {
            span.set_attribute(KeyValue::new("graphql.operation.name", name.to_string()));
        }
        span.set_attribute(KeyValue::new(
            "graphql.operation.type",
            operation_type.to_string(),
        ));
    }
}

/// Create a span that wraps an async operation.
pub async fn traced<F, T>(name: impl Into<String>, future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let context = SpanBuilder::new(name).start();
    let _guard = context.attach();
    let result = future.await;
    use opentelemetry::trace::TraceContextExt;
    context.span().end();
    result
}

/// Create an HTTP server span for a request.
pub fn http_server_span(method: &str, path: &str, route: Option<&str>) -> Context {
    let span_name = match route {
        Some(r) => format!("{} {}", method, r),
        None => format!("{} {}", method, path),
    };

    SpanBuilder::server(span_name)
        .attribute("http.request.method", method.to_string())
        .attribute("url.path", path.to_string())
        .start()
}

/// Create an HTTP client span for an outgoing request.
pub fn http_client_span(method: &str, url: &str) -> Context {
    let span_name = format!("{}", method);
    SpanBuilder::client(span_name)
        .attribute("http.request.method", method.to_string())
        .attribute("url.full", url.to_string())
        .start()
}

/// Create a GraphQL operation span.
pub fn graphql_span(operation_name: Option<&str>, operation_type: &str) -> Context {
    let span_name = match operation_name {
        Some(name) => format!("graphql {} {}", operation_type, name),
        None => format!("graphql {}", operation_type),
    };

    let mut builder = SpanBuilder::server(span_name)
        .attribute("graphql.operation.type", operation_type.to_string());

    if let Some(name) = operation_name {
        builder = builder.attribute("graphql.operation.name", name.to_string());
    }

    builder.start()
}
