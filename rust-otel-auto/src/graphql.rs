//! GraphQL instrumentation for automatic tracing.

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextExecute, NextParseQuery, NextResolve, ResolveInfo},
    parser::types::ExecutableDocument,
    Response, ServerError, ServerResult, Value,
};
use async_trait::async_trait;
use opentelemetry::{
    global,
    trace::{SpanKind, Status, TraceContextExt, Tracer},
    Context, KeyValue,
};
use std::sync::Arc;

/// Configuration for GraphQL tracing.
#[derive(Clone, Debug)]
pub struct GraphQLTracingConfig {
    /// Whether to trace individual field resolvers
    pub trace_resolvers: bool,
    /// Whether to record the full query document
    pub record_document: bool,
    /// Maximum length for document recording
    pub max_document_length: usize,
    /// Whether to record error details
    pub record_errors: bool,
    /// Custom tracer name
    pub tracer_name: &'static str,
}

impl Default for GraphQLTracingConfig {
    fn default() -> Self {
        Self {
            trace_resolvers: true,
            record_document: true,
            max_document_length: 1024,
            record_errors: true,
            tracer_name: "rust-otel-auto-graphql",
        }
    }
}

/// GraphQL tracing extension factory for async-graphql.
#[derive(Clone, Default)]
pub struct GraphQLTracingExtension {
    config: GraphQLTracingConfig,
}

impl GraphQLTracingExtension {
    /// Create a new extension with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new extension with custom configuration.
    pub fn with_config(config: GraphQLTracingConfig) -> Self {
        Self { config }
    }

    /// Enable or disable resolver tracing.
    pub fn trace_resolvers(mut self, enabled: bool) -> Self {
        self.config.trace_resolvers = enabled;
        self
    }

    /// Enable or disable document recording.
    pub fn record_document(mut self, enabled: bool) -> Self {
        self.config.record_document = enabled;
        self
    }

    /// Set maximum document length for recording.
    pub fn max_document_length(mut self, length: usize) -> Self {
        self.config.max_document_length = length;
        self
    }
}

impl ExtensionFactory for GraphQLTracingExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(GraphQLTracingExtensionImpl {
            config: self.config.clone(),
        })
    }
}

/// Internal extension implementation.
struct GraphQLTracingExtensionImpl {
    config: GraphQLTracingConfig,
}

#[async_trait]
impl Extension for GraphQLTracingExtensionImpl {
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &async_graphql::Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let tracer = global::tracer(self.config.tracer_name);

        let mut span = tracer
            .span_builder("graphql.parse")
            .with_kind(SpanKind::Internal)
            .start(&tracer);

        if self.config.record_document {
            let doc = if query.len() > self.config.max_document_length {
                format!("{}...", &query[..self.config.max_document_length])
            } else {
                query.to_string()
            };
            span.set_attribute(KeyValue::new("graphql.document", doc));
        }

        let result = next.run(ctx, query, variables).await;

        if let Err(ref errors) = result {
            span.set_status(Status::error("Parse error"));
            for error in errors {
                span.add_event(
                    "graphql.parse_error",
                    vec![KeyValue::new("error.message", error.message.clone())],
                );
            }
        }

        span.end();
        result
    }

    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let tracer = global::tracer(self.config.tracer_name);

        let operation_type = "operation";

        let span_name = match operation_name {
            Some(name) => format!("graphql.{} {}", operation_type, name),
            None => format!("graphql.{}", operation_type),
        };

        let mut attributes = vec![
            KeyValue::new("graphql.operation.type", operation_type),
        ];

        if let Some(name) = operation_name {
            attributes.push(KeyValue::new("graphql.operation.name", name.to_string()));
        }

        let span = tracer
            .span_builder(span_name)
            .with_kind(SpanKind::Server)
            .with_attributes(attributes)
            .start(&tracer);

        let context = Context::current().with_span(span.clone());
        let _guard = context.attach();

        let response = next.run(ctx, operation_name).await;

        if self.config.record_errors && !response.errors.is_empty() {
            context.span().set_status(Status::error("GraphQL errors"));
            context.span().set_attribute(KeyValue::new(
                "graphql.error_count",
                response.errors.len() as i64,
            ));

            for (i, error) in response.errors.iter().enumerate() {
                context.span().add_event(
                    "graphql.error",
                    vec![
                        KeyValue::new("error.index", i as i64),
                        KeyValue::new("error.message", error.message.clone()),
                    ],
                );
            }
        } else {
            context.span().set_status(Status::Ok);
        }

        context.span().end();
        response
    }

    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        if !self.config.trace_resolvers {
            return next.run(ctx, info).await;
        }

        let tracer = global::tracer(self.config.tracer_name);

        let span_name = format!("graphql.resolve {}", info.path_node);

        let span = tracer
            .span_builder(span_name)
            .with_kind(SpanKind::Internal)
            .with_attributes(vec![
                KeyValue::new("graphql.field.name", info.path_node.to_string()),
                KeyValue::new("graphql.field.type", info.return_type.to_string()),
                KeyValue::new("graphql.parent_type", info.parent_type.to_string()),
            ])
            .start(&tracer);

        let context = Context::current().with_span(span);
        let _guard = context.attach();

        let result = next.run(ctx, info).await;

        if let Err(ref error) = result {
            context.span().set_status(Status::error(error.message.clone()));
        }

        context.span().end();
        result
    }
}

/// Create a GraphQL operation span manually.
pub fn create_graphql_span(
    operation_name: Option<&str>,
    operation_type: &str,
    document: Option<&str>,
) -> Context {
    let tracer = global::tracer("rust-otel-auto-graphql");

    let span_name = match operation_name {
        Some(name) => format!("graphql.{} {}", operation_type, name),
        None => format!("graphql.{}", operation_type),
    };

    let mut attributes = vec![KeyValue::new(
        "graphql.operation.type",
        operation_type.to_string(),
    )];

    if let Some(name) = operation_name {
        attributes.push(KeyValue::new("graphql.operation.name", name.to_string()));
    }

    if let Some(doc) = document {
        let truncated = if doc.len() > 1024 {
            format!("{}...", &doc[..1024])
        } else {
            doc.to_string()
        };
        attributes.push(KeyValue::new("graphql.document", truncated));
    }

    let span = tracer
        .span_builder(span_name)
        .with_kind(SpanKind::Server)
        .with_attributes(attributes)
        .start(&tracer);

    Context::current().with_span(span)
}

/// Record a GraphQL error on the current span.
pub fn record_graphql_error(message: &str, path: Option<&str>) {
    let context = Context::current();
    let span = context.span();

    span.add_event(
        "graphql.error",
        vec![
            KeyValue::new("error.message", message.to_string()),
            KeyValue::new("error.path", path.unwrap_or("").to_string()),
        ],
    );
    span.set_status(Status::error(message.to_string()));
}
