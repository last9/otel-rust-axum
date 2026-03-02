# rust-otel-auto

Minimal-setup OpenTelemetry instrumentation for **Rust 1.74 + axum 0.6**.

Fills the gap left by `axum-tracing-opentelemetry`, which requires otel 0.29+ and Rust 1.75+. If you're pinned to Rust 1.74 or axum 0.6, this crate gives you full OTel semantic conventions with the minimum amount of wiring.

### What is automatic vs manual

| Concern | Effort |
|---|---|
| HTTP server spans (`http.method`, `http.route`, `http.status_code`, …) | **Automatic** — add `OtelLayer` once to your router |
| Incoming `traceparent` extraction (join an upstream trace) | **Automatic** — built into `OtelLayer` |
| Outgoing `traceparent` injection | **One-time** — use `TracedClient` instead of `reqwest::Client` |
| DB spans | **Manual per query** — call `db::sqlite_span()` around each query |
| Log–trace correlation | **Opt-in per log line** — add `trace_id = %current_trace_id()` field |

Rust has no runtime agent model (unlike the Java OTel agent), so there is no way to instrument DB calls without touching source code. This is true of every Rust OTel library — the official OTel contrib repo contains zero DB instrumentation crates. The `db::sqlite_span()` helper gives you correctly attributed spans with the least code possible.

| Dependency | Version |
|---|---|
| `opentelemetry` | 0.27 (MSRV 1.70) |
| `axum` | 0.6 |
| `reqwest` | 0.12 |
| Rust | 1.74 |

## Quick Start

```toml
# Cargo.toml
[dependencies]
rust-otel-auto = { git = "https://github.com/last9/rust-opentelemetry", features = ["axum", "reqwest", "db"] }
axum = "0.6"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"

# These two pins are required on Rust < 1.83.
# url 2.5.1+ and idna 1.0+ pull in icu4x which requires Rust 1.83.
url = "=2.5.0"
idna = "=0.5.0"
```

```rust
use axum::{Router, middleware, routing::get};
use rust_otel_auto::{init, current_trace_id};
use rust_otel_auto::layer::{OtelLayer, record_matched_route};

#[tokio::main]
async fn main() {
    // _guard must be kept alive for the duration of the process.
    // Dropping it early flushes and shuts down the exporter — any spans
    // created after that point are silently discarded.
    let _guard = init().expect("telemetry init failed");

    let app = Router::new()
        .route("/users/:id", get(get_user))
        .route_layer(middleware::from_fn(record_matched_route))  // fills http.route
        .layer(OtelLayer::new());                                 // HTTP server spans

    axum::Server::bind(&"0.0.0.0:8080".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn get_user() {
    tracing::info!(trace_id = %current_trace_id(), "handling request");
}
```

## Configuration

All configuration is via standard `OTEL_*` environment variables — no code changes needed to switch environments.

| Variable | Description | Default |
|---|---|---|
| `OTEL_SERVICE_NAME` | Service name | `unknown-service` |
| `OTEL_SERVICE_VERSION` | Service version | `1.0.0` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP HTTP endpoint | `http://localhost:4318` |
| `OTEL_EXPORTER_OTLP_HEADERS` | e.g. `Authorization=Basic <creds>` | — |
| `OTEL_TRACES_SAMPLER` | `always_on`, `always_off`, `parentbased_always_on` | `parentbased_always_on` |
| `DEPLOYMENT_ENVIRONMENT` | Added as `deployment.environment` resource attribute | `production` |

### Last9

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=https://otlp.last9.io:443
export OTEL_EXPORTER_OTLP_HEADERS="Authorization=Basic <your-base64-credentials>"
export OTEL_SERVICE_NAME=my-rust-service
export OTEL_TRACES_SAMPLER=always_on
```

Get credentials from the Last9 dashboard → **Integrations** → **OpenTelemetry**.

## Features

### HTTP Server Spans (`feature = "axum"`)

`OtelLayer` instruments every request with OTel HTTP semantic conventions:

- `http.method`, `http.target`, `http.flavor`, `http.scheme`, `http.host`
- `http.route` — filled by `record_matched_route` (shows `/users/:id`, not `/users/42`)
- `http.status_code` — filled on response
- Incoming `traceparent` / `tracestate` headers are extracted automatically

**Ordering matters** — `route_layer` runs after routing so `MatchedPath` is available:

```rust
let app = Router::new()
    .route("/users/:id", get(handler))
    .route_layer(middleware::from_fn(record_matched_route))  // inner — has MatchedPath
    .layer(OtelLayer::new());                                // outer — creates the span
```

### Outgoing HTTP Tracing (`feature = "reqwest"`)

`TracedClient` wraps `reqwest::Client` and automatically:
- Creates an OTel client span (`http.method`, `http.url`, `net.peer.name`, `http.status_code`)
- Injects `traceparent` / `tracestate` into every outgoing request

```rust
use rust_otel_auto::client::TracedClient;

let data: serde_json::Value = TracedClient::new()
    .get("https://api.example.com/users")
    .bearer_auth(token)
    .send().await?
    .json().await?;
```

For use with an existing `reqwest::Client`:

```rust
use rust_otel_auto::client::inject_trace_context;

let mut headers = reqwest::header::HeaderMap::new();
inject_trace_context(&mut headers);
client.get(url).headers(headers).send().await?;
```

### GraphQL (`async-graphql`)

`async-graphql` emits spans through the `tracing` crate. These flow into your OTel
pipeline automatically via `tracing-opentelemetry` — no extra wiring beyond adding
`.extension(Tracing)` to your schema builder.

> **Important:** Use the `tracing` feature on `async-graphql`, **not** `opentelemetry`.
> The `opentelemetry` feature pins otel 0.21 which conflicts with our otel 0.27.

```toml
# Cargo.toml
async-graphql      = { version = "6.0.11", features = ["tracing"] }  # last version with axum 0.6
async-graphql-axum = "6.0.11"

# Rust 1.74 pins — async-graphql's transitive deps have drifted past MSRV
pest           = "=2.7.15"
pest_derive    = "=2.7.15"
pest_generator = "=2.7.15"
pest_meta      = "=2.7.15"
indexmap       = "=2.6.0"
```

```rust
use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema};
use async_graphql::extensions::Tracing;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{routing::post, Router, extract::State, response::Html};

struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn hello(&self) -> &str { "world" }
}

type AppSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

fn build_schema() -> AppSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .extension(Tracing)  // emits parse / validate / execute / field spans
        .finish()
}

async fn graphql_handler(State(schema): State<AppSchema>, req: GraphQLRequest) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

async fn graphiql() -> Html<String> {
    Html(async_graphql::http::GraphiQLSource::build().endpoint("/graphql").finish())
}

// In main — OtelLayer covers the HTTP POST span; Tracing extension adds child spans
let app = Router::new()
    .route("/graphql", post(graphql_handler))
    .route("/graphiql", axum::routing::get(graphiql))
    .route_layer(middleware::from_fn(record_matched_route))
    .layer(OtelLayer::new())
    .with_state(build_schema());
```

Each GraphQL request produces a trace like:
```
POST /graphql          ← HTTP server span (OtelLayer)
  └─ request          ← GraphQL operation span
       ├─ parse
       ├─ validation
       └─ execute
            ├─ field: hello
            └─ field: ...
```

### Database Spans (`feature = "db"`)

Rust has no automatic DB instrumentation — the OTel spec requires attributes (`db.system`,
`db.operation`, `db.statement`) that only you know at the call site. This is true for every
Rust OTel library; the official contrib repo has no DB crates.

The helpers here give you correctly attributed spans with the minimum boilerplate:

```rust
use rust_otel_auto::db;

// SQLite — sets db.system, db.operation, db.statement, db.sql.table
let span = db::sqlite_span("SELECT", "SELECT id, name FROM users WHERE id = ?1", "users");

// Any other database
let span = db::db_span("postgresql", "SELECT", "SELECT * FROM orders");
```

**Critical:** rusqlite is synchronous. Always create the span outside `spawn_blocking`,
move it in, and call `.enter()` inside the closure — never across an `.await`:

```rust
let span = db::sqlite_span("SELECT", SQL, "users");

tokio::task::spawn_blocking(move || {
    let _enter = span.enter();   // sync context — safe to hold the !Send guard
    // rusqlite query here
}).await?
```

Holding `span.enter()` or `span.entered()` across an `.await` makes the future `!Send`
and breaks axum's handler bound. See the [example](https://github.com/last9/opentelemetry-examples/tree/main/rust/axum) for the full pattern.

> **Using sqlx instead of rusqlite?** The [`sqlx-tracing`](https://crates.io/crates/sqlx-tracing)
> crate (v0.2.0) wraps your `sqlx::Pool` and emits spans automatically — the closest thing
> to automatic DB instrumentation available in the Rust ecosystem today.

#### PostgreSQL (tokio-postgres / deadpool-postgres)

Same pattern — `db_span` works for any database. Pass the DBMS name as the first argument:

```rust
use rust_otel_auto::db;

async fn get_orders(pool: &deadpool_postgres::Pool) -> Vec<Order> {
    const SQL: &str = "SELECT id, total FROM orders WHERE user_id = $1";
    let span = db::db_span("postgresql", "SELECT", SQL);

    let client = pool.get().await.unwrap();
    let _enter = span.enter();   // tokio-postgres is async — see note below
    client.query(SQL, &[&user_id]).await.unwrap()
        .iter().map(Order::from_row).collect()
}
```

> **Note:** `tokio-postgres` is async, so `span.enter()` is safe only if you don't hold
> the guard across an `.await`. For async DB clients, prefer `.instrument(span)`:
>
> ```rust
> use tracing::Instrument;
>
> async move {
>     client.query(SQL, &[&user_id]).await
> }
> .instrument(span)
> .await
> ```

#### MySQL (sqlx)

`sqlx-tracing` (v0.2.0) wraps `sqlx::Pool` and emits spans automatically:

```toml
sqlx         = { version = "0.7", features = ["mysql", "runtime-tokio"] }
sqlx-tracing = { version = "0.2", features = ["mysql"] }
```

```rust
use sqlx_tracing::TracingPool;

// Wrap your pool once at startup — all queries emit spans automatically
let pool = sqlx::MySqlPool::connect(&database_url).await?;
let traced_pool = TracingPool::new(pool);

// Use exactly like sqlx::Pool — spans are emitted for every query
let rows = sqlx::query_as::<_, User>("SELECT id, name FROM users")
    .fetch_all(&traced_pool)
    .await?;
```

If you need manual spans (e.g., raw `mysql_async`), use `db_span` the same way as PostgreSQL:

```rust
let span = db::db_span("mysql", "SELECT", "SELECT id, name FROM users WHERE id = ?");
async move { conn.exec::<Row, _, _>(SQL, (id,)).await }
    .instrument(span)
    .await
```

### Log–Trace Correlation

`current_trace_id()` returns the OTel trace ID of the current span as a 32-char hex string. Use it to link log lines directly to traces in Last9 APM:

```rust
tracing::info!(trace_id = %rust_otel_auto::current_trace_id(), "user created");
```

Returns `"00000000000000000000000000000000"` when called outside any span.

## Project Structure

```
rust-opentelemetry/
├── Cargo.toml                  # Workspace
└── rust-otel-auto/
    ├── Cargo.toml
    └── src/
        ├── lib.rs              # init(), current_trace_id(), feature-gated exports
        ├── sdk.rs              # OTel SDK init with otel 0.27 API, TelemetryGuard
        ├── layer.rs            # OtelLayer, record_matched_route  [feature = "axum"]
        ├── client.rs           # TracedClient, inject_trace_context  [feature = "reqwest"]
        └── db.rs               # sqlite_span, db_span  [feature = "db"]
```

## What you get in Last9

Once instrumented, every request appears as a trace in Last9 APM with:

- **Span name**: `GET /users/:id` (route template, not the resolved path)
- **HTTP attributes**: method, target, status code, host, HTTP version
- **Distributed traces**: if a downstream service also propagates W3C `traceparent`, child spans are linked automatically
- **DB spans**: nested under the request span with `db.system`, `db.operation`, `db.statement`
- **Log correlation**: every `tracing::info!` line with `trace_id = %current_trace_id()` is linkable to the trace

## Example

A full working example (axum + SQLite + outgoing HTTP + log-trace correlation) is in
[last9/opentelemetry-examples/rust/axum](https://github.com/last9/opentelemetry-examples/tree/main/rust/axum).

## License

Apache-2.0 — built by [Last9](https://last9.io).
