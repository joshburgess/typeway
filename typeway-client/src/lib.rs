//! `typeway-client` — type-safe HTTP client derived from Wayward API types.
//!
//! The client calls endpoints using the same types as the server. If the
//! server's API type changes, the client fails to compile until updated.

pub mod call;
pub mod client;
pub mod config;
pub mod error;
pub mod interceptor;
pub mod methods;
pub mod request_builder;
pub mod retry;
pub mod streaming;
pub mod tracing_interceptor;
pub mod typed_response;

pub use call::CallEndpoint;
pub use client::Client;
pub use config::ClientConfig;
pub use error::ClientError;
pub use interceptor::{RequestInterceptor, ResponseInterceptor};
pub use request_builder::RequestBuilder;
pub use retry::RetryPolicy;
pub use tracing_interceptor::with_tracing;
pub use typed_response::TypedResponse;
