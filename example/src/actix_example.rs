//! Actix-web example demonstrating auto-instrumentation.

use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use rust_otel_auto::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Application state
struct AppState {
    request_count: AtomicU64,
}

/// User data structure
#[derive(Debug, Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
    email: Option<String>,
}

/// Create user request
#[derive(Debug, Deserialize)]
struct CreateUserRequest {
    name: String,
    email: Option<String>,
}

/// Index handler
async fn index(state: web::Data<Arc<AppState>>) -> impl Responder {
    let count = state.request_count.fetch_add(1, Ordering::SeqCst);
    HttpResponse::Ok().json(serde_json::json!({
        "message": "Welcome to the Rust OTEL Example API",
        "request_number": count + 1
    }))
}

/// Health check handler
async fn health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

/// Get user by ID
async fn get_user(path: web::Path<u64>) -> impl Responder {
    let user_id = path.into_inner();

    // Simulate some database lookup
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let user = User {
        id: user_id,
        name: format!("User {}", user_id),
        email: Some(format!("user{}@example.com", user_id)),
    };

    HttpResponse::Ok().json(user)
}

/// Create a new user
async fn create_user(body: web::Json<CreateUserRequest>) -> impl Responder {
    // Simulate user creation
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let user = User {
        id: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64 % 10000,
        name: body.name.clone(),
        email: body.email.clone(),
    };

    HttpResponse::Created().json(user)
}

/// List all users
async fn list_users() -> impl Responder {
    let users = vec![
        User {
            id: 1,
            name: "Alice".to_string(),
            email: Some("alice@example.com".to_string()),
        },
        User {
            id: 2,
            name: "Bob".to_string(),
            email: Some("bob@example.com".to_string()),
        },
        User {
            id: 3,
            name: "Charlie".to_string(),
            email: None,
        },
    ];

    HttpResponse::Ok().json(users)
}

/// Delete a user
async fn delete_user(path: web::Path<u64>) -> impl Responder {
    let _user_id = path.into_inner();
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    HttpResponse::NoContent().finish()
}

/// External API call example
async fn call_external_api() -> impl Responder {
    let client = TracedClient::new();

    match client.get("https://httpbin.org/get").send().await {
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            HttpResponse::Ok().json(serde_json::json!({
                "external_status": status.as_u16(),
                "body_length": body.len()
            }))
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

/// Error example
async fn error_example() -> impl Responder {
    HttpResponse::InternalServerError().json(serde_json::json!({
        "error": "This is a simulated error"
    }))
}

/// Slow endpoint
async fn slow_endpoint() -> impl Responder {
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    HttpResponse::Ok().json(serde_json::json!({
        "message": "This took a while!"
    }))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Load .env file
    dotenvy::dotenv().ok();

    // Initialize OpenTelemetry
    let _guard = rust_otel_auto::init().expect("Failed to initialize OpenTelemetry");

    println!("Starting Actix-web example server on http://127.0.0.1:8080");
    println!("Traces will be sent to: {}", std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4318".to_string()));
    println!("Service name: {}", std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "unknown-service".to_string()));
    println!();
    println!("Available endpoints:");
    println!("  GET  /              - Index");
    println!("  GET  /health        - Health check");
    println!("  GET  /users         - List users");
    println!("  GET  /users/:id     - Get user by ID");
    println!("  POST /users         - Create user");
    println!("  DELETE /users/:id   - Delete user");
    println!("  GET  /external      - Call external API");
    println!("  GET  /error         - Error example");
    println!("  GET  /slow          - Slow endpoint (2s)");

    let state = Arc::new(AppState {
        request_count: AtomicU64::new(0),
    });

    HttpServer::new(move || {
        App::new()
            .wrap(ActixOtelMiddleware::default())
            .app_data(web::Data::new(state.clone()))
            .route("/", web::get().to(index))
            .route("/health", web::get().to(health))
            .route("/users", web::get().to(list_users))
            .route("/users", web::post().to(create_user))
            .route("/users/{id}", web::get().to(get_user))
            .route("/users/{id}", web::delete().to(delete_user))
            .route("/external", web::get().to(call_external_api))
            .route("/error", web::get().to(error_example))
            .route("/slow", web::get().to(slow_endpoint))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
