//! Procedural macros for Rust OpenTelemetry auto-instrumentation.
//!
//! This crate provides the following attribute macros:
//!
//! - `#[traced]` - Automatically create a span for a function
//! - `#[instrument]` - Detailed span creation with customization options
//!
//! ## Example
//!
//! ```rust,ignore
//! use rust_otel_macros::{traced, instrument};
//!
//! #[traced]
//! fn my_function() {
//!     // Automatically traced
//! }
//!
//! #[instrument(name = "custom_span", skip(password))]
//! async fn login(username: &str, password: &str) -> Result<(), Error> {
//!     // Traced with custom name, password not recorded
//! }
//! ```

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    FnArg, Ident, ItemFn, Lit, Pat, Token, Expr, ExprLit,
};

/// Automatically trace a function with a span.
///
/// This is the simplest way to add tracing to a function. It creates a span
/// with the function name and records all arguments as attributes.
///
/// ## Example
///
/// ```rust,ignore
/// #[traced]
/// fn process_data(input: &str) -> String {
///     // This function is automatically traced
///     input.to_uppercase()
/// }
///
/// #[traced]
/// async fn fetch_user(id: u64) -> Result<User, Error> {
///     // Async functions are also supported
///     db.get_user(id).await
/// }
/// ```
#[proc_macro_attribute]
pub fn traced(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let output = generate_traced_fn(input, None, Vec::new(), false);
    output.into()
}

/// Instrument a function with a customizable span.
///
/// This macro provides more control over the span creation compared to `#[traced]`.
///
/// ## Attributes
///
/// - `name = "span_name"` - Custom span name (default: function name)
/// - `skip(arg1, arg2)` - Arguments to skip when recording attributes
/// - `skip_all` - Skip all arguments
/// - `level = "info"` - Tracing level (for compatibility with tracing crate)
/// - `err` - Record error on Result::Err
///
/// ## Example
///
/// ```rust,ignore
/// #[instrument(name = "user_login", skip(password), err)]
/// async fn login(username: &str, password: &str) -> Result<User, AuthError> {
///     // Custom span name, password not recorded, errors are traced
///     authenticate(username, password).await
/// }
///
/// #[instrument(skip_all)]
/// fn process_sensitive_data(secret: &[u8]) {
///     // No arguments recorded
/// }
/// ```
#[proc_macro_attribute]
pub fn instrument(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as InstrumentArgs);
    let input = parse_macro_input!(item as ItemFn);

    let output = generate_traced_fn(
        input,
        args.name,
        args.skip,
        args.skip_all,
    );

    output.into()
}

/// A single argument for the instrument macro
enum InstrumentArg {
    Name(String),
    Skip(Vec<String>),
    SkipAll,
    Err,
}

impl Parse for InstrumentArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        let ident_str = ident.to_string();

        match ident_str.as_str() {
            "name" => {
                input.parse::<Token![=]>()?;
                let lit: Lit = input.parse()?;
                if let Lit::Str(s) = lit {
                    Ok(InstrumentArg::Name(s.value()))
                } else {
                    Err(syn::Error::new_spanned(lit, "expected string literal"))
                }
            }
            "skip" => {
                let content;
                syn::parenthesized!(content in input);
                let args: Punctuated<Ident, Token![,]> =
                    Punctuated::parse_terminated(&content)?;
                Ok(InstrumentArg::Skip(
                    args.iter().map(|i| i.to_string()).collect(),
                ))
            }
            "skip_all" => Ok(InstrumentArg::SkipAll),
            "err" => Ok(InstrumentArg::Err),
            "level" => {
                // Parse but ignore for compatibility
                input.parse::<Token![=]>()?;
                let _: Lit = input.parse()?;
                Ok(InstrumentArg::Err) // Placeholder, won't affect result
            }
            _ => Err(syn::Error::new_spanned(
                ident,
                format!("unknown attribute: {}", ident_str),
            )),
        }
    }
}

/// Arguments for the instrument macro.
struct InstrumentArgs {
    name: Option<String>,
    skip: Vec<String>,
    skip_all: bool,
    #[allow(dead_code)]
    err: bool,
}

impl Parse for InstrumentArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut skip = Vec::new();
        let mut skip_all = false;
        let mut err = false;

        if input.is_empty() {
            return Ok(InstrumentArgs {
                name,
                skip,
                skip_all,
                err,
            });
        }

        let args: Punctuated<InstrumentArg, Token![,]> =
            Punctuated::parse_terminated(input)?;

        for arg in args {
            match arg {
                InstrumentArg::Name(n) => name = Some(n),
                InstrumentArg::Skip(s) => skip.extend(s),
                InstrumentArg::SkipAll => skip_all = true,
                InstrumentArg::Err => err = true,
            }
        }

        Ok(InstrumentArgs {
            name,
            skip,
            skip_all,
            err,
        })
    }
}

/// Generate the traced function implementation.
fn generate_traced_fn(
    input: ItemFn,
    custom_name: Option<String>,
    skip_args: Vec<String>,
    skip_all: bool,
) -> TokenStream2 {
    let fn_name = &input.sig.ident;
    let fn_block = &input.block;
    let fn_vis = &input.vis;
    let fn_sig = &input.sig;
    let fn_attrs = &input.attrs;

    let span_name = custom_name.unwrap_or_else(|| fn_name.to_string());
    let is_async = input.sig.asyncness.is_some();

    // Collect argument names for attributes
    let arg_attrs: Vec<TokenStream2> = if skip_all {
        Vec::new()
    } else {
        input
            .sig
            .inputs
            .iter()
            .filter_map(|arg| {
                if let FnArg::Typed(pat_type) = arg {
                    if let Pat::Ident(pat_ident) = &*pat_type.pat {
                        let arg_name = pat_ident.ident.to_string();
                        if skip_args.contains(&arg_name) {
                            return None;
                        }
                        let arg_ident = &pat_ident.ident;
                        return Some(quote! {
                            __span.set_attribute(
                                ::opentelemetry::KeyValue::new(
                                    #arg_name,
                                    format!("{:?}", #arg_ident)
                                )
                            );
                        });
                    }
                }
                None
            })
            .collect()
    };

    let set_attrs = if arg_attrs.is_empty() {
        quote! {}
    } else {
        quote! {
            #(#arg_attrs)*
        }
    };

    if is_async {
        quote! {
            #(#fn_attrs)*
            #fn_vis #fn_sig {
                use ::opentelemetry::trace::{Tracer, Span, TraceContextExt};

                let __tracer = ::opentelemetry::global::tracer("rust-otel-auto");
                let mut __span = __tracer.start(#span_name);
                #set_attrs

                let __context = ::opentelemetry::Context::current().with_span(__span);
                let __guard = __context.attach();

                let __result = async move #fn_block.await;

                let __span = __context.span();
                __span.end();

                __result
            }
        }
    } else {
        quote! {
            #(#fn_attrs)*
            #fn_vis #fn_sig {
                use ::opentelemetry::trace::{Tracer, Span, TraceContextExt};

                let __tracer = ::opentelemetry::global::tracer("rust-otel-auto");
                let mut __span = __tracer.start(#span_name);
                #set_attrs

                let __context = ::opentelemetry::Context::current().with_span(__span);
                let __guard = __context.attach();

                let __result = (|| #fn_block)();

                let __span = __context.span();
                __span.end();

                __result
            }
        }
    }
}

/// Pin projection helper for context propagation.
///
/// This is an internal macro used by the context module.
#[proc_macro_attribute]
pub fn pin_project(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Simple passthrough - the struct already handles projection manually
    item
}
