# Rust OpenTelemetry Auto-Instrumentation

A comprehensive auto-instrumentation library for Rust applications with OpenTelemetry support. Designed to work with **Rust 1.74+** while the official OpenTelemetry Rust SDK requires 1.75+.

## Features

- **Zero-code instrumentation**: Automatic tracing with minimal code changes
- **HTTP Server Middleware**: Support for Actix-web and Axum frameworks
- **GraphQL Support**: Automatic instrumentation for async-graphql
- **HTTP Client Tracing**: Automatic instrumentation for reqwest
- **Context Propagation**: W3C Trace Context propagation for distributed tracing
- **Environment Configuration**: Standard OTEL_* environment variable support
- **Procedural Macros**: `#[traced]` and `#[instrument]` attributes for easy instrumentation

## Quick Start

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
rust-otel-auto = { path = "path/to/rust-otel-auto", features = ["full"] }
tokio = { version = "1", features = ["full"] }
```

### Basic Usage

```rust
use rust_otel_auto::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize OpenTelemetry with auto-configuration from environment
    let _guard = rust_otel_auto::init()?;

    // Your application code here
    // The guard must be kept alive for the SDK to function

    Ok(())
}
```

### Configuration

Configure via environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `OTEL_SERVICE_NAME` | Service name for traces | `unknown-service` |
| `OTEL_SERVICE_VERSION` | Service version | `1.0.0` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP endpoint (Last9: `https://otlp.last9.io:443`) | `http://localhost:4318` |
| `OTEL_EXPORTER_OTLP_HEADERS` | Headers for OTLP exporter (Last9: `Authorization=Basic <credentials>`) | - |
| `OTEL_TRACES_SAMPLER` | Sampling strategy | `parentbased_always_on` |
| `OTEL_TRACES_SAMPLER_ARG` | Sampler argument | `1.0` |
| `DEPLOYMENT_ENVIRONMENT` | Deployment environment | `production` |
| `OTEL_RESOURCE_ATTRIBUTES` | Additional resource attributes | - |

#### Last9 Configuration

For sending traces to [Last9](https://last9.io):

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=https://otlp.last9.io:443
export OTEL_EXPORTER_OTLP_HEADERS="Authorization=Basic <your-base64-credentials>"
export OTEL_SERVICE_NAME=my-rust-service
```

Get your credentials from the Last9 dashboard under **Integrations** → **OpenTelemetry**.

Or use programmatic configuration:

```rust
use rust_otel_auto::prelude::*;

let config = OtelConfig::builder()
    .service_name("my-service")
    .service_version("1.0.0")
    .endpoint("http://collector:4318")
    .sampler(Sampler::TraceIdRatio(0.5))
    .build();

let _guard = rust_otel_auto::init_with_config(config)?;
```

## HTTP Server Instrumentation

### Actix-web

```rust
use actix_web::{web, App, HttpServer};
use rust_otel_auto::prelude::*;

#[traced]
async fn index() -> &'static str {
    "Hello, World!"
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let _guard = rust_otel_auto::init().unwrap();

    HttpServer::new(|| {
        App::new()
            .wrap(ActixOtelMiddleware::default())
            .route("/", web::get().to(index))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
```

### Axum

```rust
use axum::{Router, routing::get};
use rust_otel_auto::prelude::*;

#[traced]
async fn index() -> &'static str {
    "Hello, World!"
}

#[tokio::main]
async fn main() {
    let _guard = rust_otel_auto::init().unwrap();

    let app = Router::new()
        .route("/", get(index))
        .layer(AxumOtelLayer::default());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

## GraphQL Instrumentation

```rust
use async_graphql::{EmptyMutation, EmptySubscription, Schema, Object};
use rust_otel_auto::graphql::GraphQLTracingExtension;

struct Query;

#[Object]
impl Query {
    async fn hello(&self) -> &str {
        "Hello, GraphQL!"
    }
}

let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
    .extension(GraphQLTracingExtension::new()
        .trace_resolvers(true)
        .record_document(true))
    .finish();
```

## HTTP Client Instrumentation

```rust
use rust_otel_auto::prelude::*;

#[traced]
async fn fetch_data() -> Result<String, reqwest::Error> {
    let client = TracedClient::new();

    let response = client
        .get("https://api.example.com/data")
        .header("Accept", "application/json")
        .send()
        .await?;

    response.text().await
}
```

## Function Instrumentation

### Using `#[traced]`

```rust
use rust_otel_auto::prelude::*;

#[traced]
fn process_data(input: &str) -> String {
    input.to_uppercase()
}

#[traced]
async fn async_operation() {
    // Async functions are also supported
}
```

### Using `#[instrument]`

```rust
use rust_otel_auto::prelude::*;

#[instrument(name = "user_login", skip(password))]
async fn login(username: &str, password: &str) -> Result<User, Error> {
    // Custom span name, password not recorded
    authenticate(username, password).await
}

#[instrument(skip_all)]
fn process_sensitive_data(secret: &[u8]) {
    // No arguments recorded
}
```

### Using the `trace_span!` macro

```rust
use rust_otel_auto::trace_span;

fn my_function() {
    let _span = trace_span!("my_operation");
    // Do work...
}

fn with_attributes() {
    let _span = trace_span!(
        "database_query",
        "db.system" => "postgresql",
        "db.operation" => "SELECT"
    );
    // Do work...
}
```

## Context Propagation

For distributed tracing across services:

```rust
use rust_otel_auto::context::{extract_context, inject_context, spawn_with_context};

// Extract context from incoming headers
let parent_context = extract_context(headers.iter());

// Inject context into outgoing headers
inject_context(|key, value| {
    headers.insert(key, value);
});

// Spawn a task with context propagation
spawn_with_context(async {
    // This task has the parent's trace context
});
```

## Project Structure

```
rust-opentelemetry/
├── Cargo.toml              # Workspace definition
├── rust-otel-auto/         # Main auto-instrumentation library
│   ├── src/
│   │   ├── lib.rs          # Library entry point
│   │   ├── config.rs       # Configuration management
│   │   ├── sdk.rs          # SDK initialization
│   │   ├── context.rs      # Context propagation
│   │   ├── propagator.rs   # W3C Trace Context
│   │   ├── span.rs         # Span utilities
│   │   ├── graphql.rs      # GraphQL instrumentation
│   │   ├── reqwest_ext.rs  # HTTP client instrumentation
│   │   └── middleware/     # HTTP framework middleware
│   │       ├── mod.rs
│   │       ├── actix.rs    # Actix-web middleware
│   │       └── axum.rs     # Axum middleware
│   └── Cargo.toml
├── rust-otel-macros/       # Procedural macros
│   ├── src/lib.rs
│   └── Cargo.toml
└── example/                # Example applications
    ├── src/
    │   ├── actix_example.rs   # Actix-web HTTP server example
    │   └── graphql_example.rs # async-graphql example
    ├── .env.example           # Last9 configuration template
    └── Cargo.toml
```

## Running the Examples

### 1. Configure Last9 Credentials

1. Sign up at [Last9](https://last9.io) and create an account
2. Navigate to **Integrations** → **OpenTelemetry** in your Last9 dashboard
3. Copy your OTLP endpoint and credentials
4. Configure your environment:

```bash
cd example
cp .env.example .env
```

Edit `.env` with your Last9 credentials:

```env
# Last9 OTLP Endpoint (check your region in Last9 dashboard)
OTEL_EXPORTER_OTLP_ENDPOINT=https://otlp.last9.io:443

# Basic Auth Header from Last9 dashboard
# Format: Authorization=Basic <base64(username:password)>
OTEL_EXPORTER_OTLP_HEADERS=Authorization=Basic YOUR_BASE64_CREDENTIALS

# Service identification
OTEL_SERVICE_NAME=rust-otel-example
OTEL_SERVICE_VERSION=1.0.0
DEPLOYMENT_ENVIRONMENT=development
```

### 2. Run an Example

```bash
# Actix-web example
cargo run --bin actix-example

# GraphQL example
cargo run --bin graphql-example
```

### 3. Make Requests

```bash
# HTTP endpoints
curl http://localhost:8080/
curl http://localhost:8080/health
curl http://localhost:8080/users
curl http://localhost:8080/users/123

# External API call (demonstrates HTTP client tracing)
curl http://localhost:8080/external

# GraphQL
curl -X POST http://localhost:8080/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ users { id name } }"}'
```

### 4. View Traces in Last9

1. Open your [Last9 Dashboard](https://app.last9.io)
2. Navigate to **APM** → **Traces**
3. Filter by your service name (e.g., `rust-otel-example`)
4. Explore distributed traces, spans, and performance metrics

## Comparison with Official OpenTelemetry Rust

| Feature | rust-otel-auto | opentelemetry-rust |
|---------|----------------|-------------------|
| Minimum Rust Version | 1.74 | 1.75 |
| Auto-instrumentation | Yes | No (manual only) |
| HTTP Middleware | Built-in | Separate crates |
| GraphQL Support | Built-in | Not available |
| Proc Macros | `#[traced]`, `#[instrument]` | None |
| Configuration | Environment + Builder | Manual setup |
| Last9 Integration | Built-in | Manual setup |

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## License

Apache-2.0

## Acknowledgements

Built with ❤️ by [Last9](https://last9.io) - the observability platform for SRE teams.

This library was inspired by:
- [OpenTelemetry Rust](https://github.com/open-telemetry/opentelemetry-rust)
- [Last9 Vert.x OpenTelemetry](https://github.com/last9/vertx-opentelemetry)
- [Last9 OpenResty OTEL](https://github.com/last9/openresty-otel)

## Support

- [Last9 Documentation](https://docs.last9.io)
- [Last9 Community](https://last9.io/community)
- [GitHub Issues](https://github.com/last9/rust-opentelemetry/issues)
