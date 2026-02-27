//! GraphQL example demonstrating auto-instrumentation.

use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use async_graphql::{
    http::GraphiQLSource, Context, EmptySubscription, Object, Schema, SimpleObject, ID,
};
use async_graphql_actix_web::{GraphQLRequest, GraphQLResponse};
use rust_otel_auto::graphql::GraphQLTracingExtension;
use rust_otel_auto::middleware::actix::OtelMiddleware as ActixOtelMiddleware;
use std::sync::{Arc, Mutex};

/// User entity
#[derive(Clone, SimpleObject)]
struct User {
    id: ID,
    name: String,
    email: Option<String>,
}

/// Blog post entity
#[derive(Clone, SimpleObject)]
struct Post {
    id: ID,
    title: String,
    content: String,
    author_id: ID,
}

/// Application data store
struct DataStore {
    users: Vec<User>,
    posts: Vec<Post>,
}

impl Default for DataStore {
    fn default() -> Self {
        Self {
            users: vec![
                User {
                    id: ID::from("1"),
                    name: "Alice".to_string(),
                    email: Some("alice@example.com".to_string()),
                },
                User {
                    id: ID::from("2"),
                    name: "Bob".to_string(),
                    email: Some("bob@example.com".to_string()),
                },
                User {
                    id: ID::from("3"),
                    name: "Charlie".to_string(),
                    email: None,
                },
            ],
            posts: vec![
                Post {
                    id: ID::from("1"),
                    title: "Introduction to GraphQL".to_string(),
                    content: "GraphQL is a query language for APIs...".to_string(),
                    author_id: ID::from("1"),
                },
                Post {
                    id: ID::from("2"),
                    title: "OpenTelemetry Best Practices".to_string(),
                    content: "When implementing distributed tracing...".to_string(),
                    author_id: ID::from("1"),
                },
                Post {
                    id: ID::from("3"),
                    title: "Rust Performance Tips".to_string(),
                    content: "Rust provides zero-cost abstractions...".to_string(),
                    author_id: ID::from("2"),
                },
            ],
        }
    }
}

/// GraphQL Query root
struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Get all users
    async fn users(&self, ctx: &Context<'_>) -> Vec<User> {
        let store = ctx.data_unchecked::<Arc<Mutex<DataStore>>>();
        let store = store.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        store.users.clone()
    }

    /// Get a user by ID
    async fn user(&self, ctx: &Context<'_>, id: i32) -> Option<User> {
        let store = ctx.data_unchecked::<Arc<Mutex<DataStore>>>();
        let store = store.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
        let id_str = id.to_string();
        store.users.iter().find(|u| u.id.as_str() == id_str).cloned()
    }

    /// Get all posts
    async fn posts(&self, ctx: &Context<'_>) -> Vec<Post> {
        let store = ctx.data_unchecked::<Arc<Mutex<DataStore>>>();
        let store = store.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(15)).await;
        store.posts.clone()
    }

    /// Get posts by a specific user
    async fn posts_by_user(&self, ctx: &Context<'_>, user_id: i32) -> Vec<Post> {
        let store = ctx.data_unchecked::<Arc<Mutex<DataStore>>>();
        let store = store.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(25)).await;
        let user_id_str = user_id.to_string();
        store
            .posts
            .iter()
            .filter(|p| p.author_id.as_str() == user_id_str)
            .cloned()
            .collect()
    }

    /// Search users by name
    async fn search_users(&self, ctx: &Context<'_>, query: String) -> Vec<User> {
        let store = ctx.data_unchecked::<Arc<Mutex<DataStore>>>();
        let store = store.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;
        let query_lower = query.to_lowercase();
        store
            .users
            .iter()
            .filter(|u| u.name.to_lowercase().contains(&query_lower))
            .cloned()
            .collect()
    }
}

/// GraphQL Mutation root
struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Create a new user
    async fn create_user(
        &self,
        ctx: &Context<'_>,
        name: String,
        email: Option<String>,
    ) -> User {
        let store = ctx.data_unchecked::<Arc<Mutex<DataStore>>>();
        let mut store = store.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let new_id = (store.users.len() + 1).to_string();
        let user = User {
            id: ID::from(new_id),
            name,
            email,
        };
        store.users.push(user.clone());
        user
    }

    /// Create a new post
    async fn create_post(
        &self,
        ctx: &Context<'_>,
        title: String,
        content: String,
        author_id: i32,
    ) -> Post {
        let store = ctx.data_unchecked::<Arc<Mutex<DataStore>>>();
        let mut store = store.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;
        let new_id = (store.posts.len() + 1).to_string();
        let post = Post {
            id: ID::from(new_id),
            title,
            content,
            author_id: ID::from(author_id.to_string()),
        };
        store.posts.push(post.clone());
        post
    }

    /// Delete a user
    async fn delete_user(&self, ctx: &Context<'_>, id: i32) -> bool {
        let store = ctx.data_unchecked::<Arc<Mutex<DataStore>>>();
        let mut store = store.lock().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
        let id_str = id.to_string();
        let initial_len = store.users.len();
        store.users.retain(|u| u.id.as_str() != id_str);
        store.users.len() < initial_len
    }
}

/// GraphQL handler
async fn graphql_handler(
    schema: web::Data<Schema<QueryRoot, MutationRoot, EmptySubscription>>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

/// GraphiQL playground handler
async fn graphiql() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(GraphiQLSource::build().endpoint("/graphql").finish())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Load .env file
    dotenvy::dotenv().ok();

    // Initialize OpenTelemetry
    let _guard = rust_otel_auto::init().expect("Failed to initialize OpenTelemetry");

    println!("Starting GraphQL example server on http://127.0.0.1:8080");
    println!("Traces will be sent to: {}", std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4318".to_string()));
    println!("Service name: {}", std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "unknown-service".to_string()));
    println!();
    println!("Endpoints:");
    println!("  POST /graphql      - GraphQL endpoint");
    println!("  GET  /playground   - GraphQL Playground");
    println!();
    println!("Example queries:");
    println!("  query {{ users {{ id name email }} }}");
    println!("  query {{ user(id: 1) {{ id name }} }}");
    println!("  mutation {{ createUser(name: \"Dave\", email: \"dave@example.com\") {{ id }} }}");

    let store = Arc::new(Mutex::new(DataStore::default()));

    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .extension(GraphQLTracingExtension::new().trace_resolvers(true))
        .data(store)
        .finish();

    HttpServer::new(move || {
        App::new()
            .wrap(ActixOtelMiddleware::default())
            .app_data(web::Data::new(schema.clone()))
            .route("/graphql", web::post().to(graphql_handler))
            .route("/playground", web::get().to(graphiql))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
