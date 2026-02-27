//! Reqwest HTTP client instrumentation for automatic tracing.

use opentelemetry::{
    global,
    trace::{SpanKind, Status, TraceContextExt, Tracer},
    Context, KeyValue,
};
use opentelemetry_semantic_conventions::trace::{
    HTTP_REQUEST_METHOD, HTTP_RESPONSE_STATUS_CODE, SERVER_ADDRESS, SERVER_PORT, URL_FULL,
};
use reqwest::{header::HeaderMap, Client, Method, RequestBuilder, Response, Url};
use std::time::Duration;

/// A traced HTTP client that wraps reqwest::Client.
#[derive(Clone, Debug)]
pub struct TracedClient {
    inner: Client,
    tracer_name: &'static str,
}

impl Default for TracedClient {
    fn default() -> Self {
        Self::new()
    }
}

impl TracedClient {
    /// Create a new traced client with default configuration.
    pub fn new() -> Self {
        Self {
            inner: Client::new(),
            tracer_name: "rust-otel-auto-http-client",
        }
    }

    /// Create a traced client from an existing reqwest::Client.
    pub fn from_client(client: Client) -> Self {
        Self {
            inner: client,
            tracer_name: "rust-otel-auto-http-client",
        }
    }

    /// Create a new traced client with a builder.
    pub fn builder() -> TracedClientBuilder {
        TracedClientBuilder::new()
    }

    /// Set a custom tracer name.
    pub fn with_tracer_name(mut self, name: &'static str) -> Self {
        self.tracer_name = name;
        self
    }

    /// Start building a GET request.
    pub fn get(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder::new(
            self.inner.get(url.as_ref()),
            Method::GET,
            url.as_ref().to_string(),
            self.tracer_name,
        )
    }

    /// Start building a POST request.
    pub fn post(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder::new(
            self.inner.post(url.as_ref()),
            Method::POST,
            url.as_ref().to_string(),
            self.tracer_name,
        )
    }

    /// Start building a PUT request.
    pub fn put(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder::new(
            self.inner.put(url.as_ref()),
            Method::PUT,
            url.as_ref().to_string(),
            self.tracer_name,
        )
    }

    /// Start building a PATCH request.
    pub fn patch(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder::new(
            self.inner.patch(url.as_ref()),
            Method::PATCH,
            url.as_ref().to_string(),
            self.tracer_name,
        )
    }

    /// Start building a DELETE request.
    pub fn delete(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder::new(
            self.inner.delete(url.as_ref()),
            Method::DELETE,
            url.as_ref().to_string(),
            self.tracer_name,
        )
    }

    /// Start building a HEAD request.
    pub fn head(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder::new(
            self.inner.head(url.as_ref()),
            Method::HEAD,
            url.as_ref().to_string(),
            self.tracer_name,
        )
    }

    /// Start building a request with a custom method.
    pub fn request(&self, method: Method, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder::new(
            self.inner.request(method.clone(), url.as_ref()),
            method,
            url.as_ref().to_string(),
            self.tracer_name,
        )
    }

    /// Get the underlying reqwest client.
    pub fn inner(&self) -> &Client {
        &self.inner
    }
}

/// Builder for TracedClient.
pub struct TracedClientBuilder {
    builder: reqwest::ClientBuilder,
    tracer_name: &'static str,
}

impl TracedClientBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            builder: Client::builder(),
            tracer_name: "rust-otel-auto-http-client",
        }
    }

    /// Set a custom tracer name.
    pub fn tracer_name(mut self, name: &'static str) -> Self {
        self.tracer_name = name;
        self
    }

    /// Set the request timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.builder = self.builder.timeout(timeout);
        self
    }

    /// Set the connection timeout.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.builder = self.builder.connect_timeout(timeout);
        self
    }

    /// Set default headers.
    pub fn default_headers(mut self, headers: HeaderMap) -> Self {
        self.builder = self.builder.default_headers(headers);
        self
    }

    /// Set the user agent.
    pub fn user_agent(mut self, user_agent: impl AsRef<str>) -> Self {
        self.builder = self.builder.user_agent(user_agent.as_ref());
        self
    }

    /// Build the traced client.
    pub fn build(self) -> Result<TracedClient, reqwest::Error> {
        Ok(TracedClient {
            inner: self.builder.build()?,
            tracer_name: self.tracer_name,
        })
    }
}

impl Default for TracedClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A traced request builder.
pub struct TracedRequestBuilder {
    inner: RequestBuilder,
    method: Method,
    url: String,
    tracer_name: &'static str,
}

impl TracedRequestBuilder {
    fn new(
        inner: RequestBuilder,
        method: Method,
        url: String,
        tracer_name: &'static str,
    ) -> Self {
        Self {
            inner,
            method,
            url,
            tracer_name,
        }
    }

    /// Add a header to the request.
    pub fn header(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.inner = self.inner.header(key.as_ref(), value.as_ref());
        self
    }

    /// Add headers to the request.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.inner = self.inner.headers(headers);
        self
    }

    /// Set the request body.
    pub fn body(mut self, body: impl Into<reqwest::Body>) -> Self {
        self.inner = self.inner.body(body);
        self
    }

    /// Set a JSON body.
    pub fn json<T: serde::Serialize + ?Sized>(mut self, json: &T) -> Self {
        self.inner = self.inner.json(json);
        self
    }

    /// Set a form body.
    pub fn form<T: serde::Serialize + ?Sized>(mut self, form: &T) -> Self {
        self.inner = self.inner.form(form);
        self
    }

    /// Set query parameters.
    pub fn query<T: serde::Serialize + ?Sized>(mut self, query: &T) -> Self {
        self.inner = self.inner.query(query);
        self
    }

    /// Set a timeout for this request.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    /// Set bearer auth.
    pub fn bearer_auth(mut self, token: impl AsRef<str>) -> Self {
        self.inner = self.inner.bearer_auth(token.as_ref());
        self
    }

    /// Set basic auth.
    pub fn basic_auth(
        mut self,
        username: impl AsRef<str>,
        password: Option<impl AsRef<str>>,
    ) -> Self {
        self.inner = self.inner.basic_auth(
            username.as_ref(),
            password.as_ref().map(|p| p.as_ref()),
        );
        self
    }

    /// Send the request.
    pub async fn send(self) -> Result<Response, reqwest::Error> {
        let tracer = global::tracer(self.tracer_name);

        // Parse URL for attributes
        let url_parsed = Url::parse(&self.url).ok();
        let host = url_parsed
            .as_ref()
            .and_then(|u| u.host_str())
            .unwrap_or("unknown");
        let port = url_parsed.as_ref().and_then(|u| u.port());

        // Create span name
        let span_name = format!("{} {}", self.method, host);

        // Build span attributes
        let mut attributes = vec![
            KeyValue::new(HTTP_REQUEST_METHOD, self.method.to_string()),
            KeyValue::new(URL_FULL, self.url.clone()),
            KeyValue::new(SERVER_ADDRESS, host.to_string()),
        ];

        if let Some(p) = port {
            attributes.push(KeyValue::new(SERVER_PORT, p as i64));
        }

        // Create the span
        let span = tracer
            .span_builder(span_name)
            .with_kind(SpanKind::Client)
            .with_attributes(attributes)
            .start(&tracer);

        // Inject trace context into request headers
        let context = Context::current().with_span(span);
        let request = inject_trace_context(self.inner, &context);

        // Attach context
        let _guard = context.clone().attach();

        // Execute the request
        let result = request.send().await;

        // Record response details
        match &result {
            Ok(response) => {
                let status_code = response.status().as_u16();
                context.span().set_attribute(KeyValue::new(
                    HTTP_RESPONSE_STATUS_CODE,
                    status_code as i64,
                ));

                if status_code >= 400 {
                    context.span().set_status(Status::error(format!("HTTP {}", status_code)));
                } else {
                    context.span().set_status(Status::Ok);
                }
            }
            Err(error) => {
                context.span().set_status(Status::error(error.to_string()));
            }
        }

        context.span().end();
        result
    }
}

/// Inject trace context into request headers.
fn inject_trace_context(request: RequestBuilder, context: &Context) -> RequestBuilder {
    let mut headers = Vec::new();
    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(context, &mut |key, value| {
            headers.push((key.to_string(), value));
        });
    });

    let mut req = request;
    for (key, value) in headers {
        req = req.header(&key, &value);
    }
    req
}

/// Trait to add tracing to existing reqwest clients.
pub trait ClientExt {
    /// Create a traced GET request.
    fn traced_get(&self, url: impl AsRef<str>) -> TracedRequestBuilder;
    /// Create a traced POST request.
    fn traced_post(&self, url: impl AsRef<str>) -> TracedRequestBuilder;
    /// Create a traced PUT request.
    fn traced_put(&self, url: impl AsRef<str>) -> TracedRequestBuilder;
    /// Create a traced DELETE request.
    fn traced_delete(&self, url: impl AsRef<str>) -> TracedRequestBuilder;
}

impl ClientExt for Client {
    fn traced_get(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder::new(
            self.get(url.as_ref()),
            Method::GET,
            url.as_ref().to_string(),
            "rust-otel-auto-http-client",
        )
    }

    fn traced_post(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder::new(
            self.post(url.as_ref()),
            Method::POST,
            url.as_ref().to_string(),
            "rust-otel-auto-http-client",
        )
    }

    fn traced_put(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder::new(
            self.put(url.as_ref()),
            Method::PUT,
            url.as_ref().to_string(),
            "rust-otel-auto-http-client",
        )
    }

    fn traced_delete(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder::new(
            self.delete(url.as_ref()),
            Method::DELETE,
            url.as_ref().to_string(),
            "rust-otel-auto-http-client",
        )
    }
}
