//! Database span helpers with OTel semantic conventions.
//!
//! These functions return a `tracing::Span` pre-populated with the correct
//! `db.*` attributes. Enter the span before your database call:
//!
//! ```rust,no_run
//! use rust_otel_auto::db;
//!
//! async fn fetch_users(db: Arc<Mutex<rusqlite::Connection>>) -> Vec<User> {
//!     const SQL: &str = "SELECT id, name FROM users";
//!     let _span = db::sqlite_span("SELECT", SQL, "users").entered();
//!
//!     tokio::task::spawn_blocking(move || {
//!         let conn = db.lock().unwrap();
//!         // ... run query ...
//!     }).await.unwrap()
//! }
//! ```

/// Creates an OTel span for a SQLite query.
///
/// Sets: `db.system = "sqlite"`, `db.operation`, `db.statement`, `db.sql.table`, `db.name = ":memory:"`.
/// Override `db.name` by calling `span.record("db.name", "path/to/db.sqlite")` after creation.
pub fn sqlite_span(
    operation: &'static str,
    statement: &'static str,
    table: &'static str,
) -> tracing::Span {
    tracing::info_span!(
        "db.query",
        "otel.kind"    = "client",
        "db.system"    = "sqlite",
        "db.operation" = operation,
        "db.statement" = statement,
        "db.sql.table" = table,
        "db.name"      = ":memory:",
    )
}

/// Creates an OTel span for any database query.
///
/// Sets: `db.system`, `db.operation`, `db.statement`.
/// Add extra attributes via `span.record("db.name", ...)` etc. after creation.
pub fn db_span(
    system: &'static str,
    operation: &'static str,
    statement: &'static str,
) -> tracing::Span {
    tracing::info_span!(
        "db.query",
        "otel.kind"    = "client",
        "db.system"    = system,
        "db.operation" = operation,
        "db.statement" = statement,
    )
}
