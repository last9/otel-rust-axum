//! Reqwest 0.12 HTTP client instrumentation with OTel semantic conventions.
//!
//! # Usage
//!
//! ```rust,no_run
//! use otel_rust_axum::client::TracedClient;
//!
//! let client = TracedClient::new();
//! let resp = client.get("https://api.example.com/data").send().await?;
//! ```

use opentelemetry::{global, propagation::Injector, Context};
use reqwest::{header::HeaderMap, Client, Method, RequestBuilder, Response};
use std::time::Duration;
use tracing::{Instrument, Span};

// ---------------------------------------------------------------------------
// W3C TraceContext injector for outgoing requests
// ---------------------------------------------------------------------------

struct HeaderInjector<'a>(&'a mut HeaderMap);

impl<'a> Injector for HeaderInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(val)) = (
            reqwest::header::HeaderName::from_bytes(key.as_bytes()),
            reqwest::header::HeaderValue::from_str(&value),
        ) {
            self.0.insert(name, val);
        }
    }
}

/// Inject the current OTel trace context into a `HeaderMap` as a `traceparent` header.
///
/// Use this when building requests manually with an existing `reqwest::Client`:
/// ```rust,no_run
/// let mut headers = reqwest::header::HeaderMap::new();
/// otel_rust_axum::client::inject_trace_context(&mut headers);
/// client.get(url).headers(headers).send().await?;
/// ```
pub fn inject_trace_context(headers: &mut HeaderMap) {
    let cx = Context::current();
    global::get_text_map_propagator(|p| p.inject_context(&cx, &mut HeaderInjector(headers)));
}

// ---------------------------------------------------------------------------
// TracedClient
// ---------------------------------------------------------------------------

/// A `reqwest::Client` wrapper that automatically:
/// - Injects `traceparent` / `tracestate` headers into every outgoing request
/// - Creates an OTel HTTP client span with semantic convention attributes
///
/// # Example
/// ```rust,no_run
/// let client = otel_rust_axum::client::TracedClient::new();
/// let data: serde_json::Value = client
///     .get("https://api.example.com/users")
///     .send().await?
///     .json().await?;
/// ```
#[derive(Clone, Debug)]
pub struct TracedClient {
    inner: Client,
}

impl Default for TracedClient {
    fn default() -> Self {
        Self::new()
    }
}

impl TracedClient {
    /// Create a new `TracedClient` with default settings.
    pub fn new() -> Self {
        Self { inner: Client::new() }
    }

    /// Wrap an existing `reqwest::Client`.
    pub fn from_client(client: Client) -> Self {
        Self { inner: client }
    }

    /// Access the underlying `reqwest::Client`.
    pub fn inner(&self) -> &Client {
        &self.inner
    }

    pub fn get(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        self.request(Method::GET, url)
    }

    pub fn post(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        self.request(Method::POST, url)
    }

    pub fn put(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        self.request(Method::PUT, url)
    }

    pub fn patch(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        self.request(Method::PATCH, url)
    }

    pub fn delete(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        self.request(Method::DELETE, url)
    }

    pub fn head(&self, url: impl AsRef<str>) -> TracedRequestBuilder {
        self.request(Method::HEAD, url)
    }

    pub fn request(&self, method: Method, url: impl AsRef<str>) -> TracedRequestBuilder {
        TracedRequestBuilder {
            inner:  self.inner.request(method.clone(), url.as_ref()),
            method,
            url:    url.as_ref().to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// TracedRequestBuilder
// ---------------------------------------------------------------------------

/// A request builder that adds an OTel client span and injects trace context on `send()`.
pub struct TracedRequestBuilder {
    inner:  RequestBuilder,
    method: Method,
    url:    String,
}

impl TracedRequestBuilder {
    pub fn header(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.inner = self.inner.header(key.as_ref(), value.as_ref());
        self
    }

    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.inner = self.inner.headers(headers);
        self
    }

    pub fn json<T: serde::Serialize + ?Sized>(mut self, body: &T) -> Self {
        self.inner = self.inner.json(body);
        self
    }

    pub fn query<T: serde::Serialize + ?Sized>(mut self, query: &T) -> Self {
        self.inner = self.inner.query(query);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    pub fn bearer_auth(mut self, token: impl AsRef<str>) -> Self {
        self.inner = self.inner.bearer_auth(token.as_ref());
        self
    }

    /// Send the request.
    ///
    /// Creates an OTel HTTP client span with `http.method`, `http.url`, `net.peer.name`,
    /// and `http.status_code`, and injects `traceparent` into the request headers.
    pub async fn send(self) -> Result<Response, reqwest::Error> {
        let peer = reqwest::Url::parse(&self.url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_default();

        let span = tracing::info_span!(
            "HTTP client",
            "otel.kind"         = "client",
            "otel.name"         = format!("{} {}", self.method, peer).as_str(),
            "http.method"       = self.method.as_str(),
            "http.url"          = self.url.as_str(),
            "net.peer.name"     = peer.as_str(),
            "http.status_code"  = tracing::field::Empty,
        );

        // Inject trace context then send — wrapped with .instrument(span) so no
        // !Send span guard is held across the .await; the guard is entered/exited
        // on each poll by the Instrumented wrapper, keeping the future Send.
        async move {
            let mut headers = HeaderMap::new();
            inject_trace_context(&mut headers);

            let response = self.inner.headers(headers).send().await?;

            Span::current().record("http.status_code", response.status().as_u16());

            Ok(response)
        }
        .instrument(span)
        .await
    }
}
