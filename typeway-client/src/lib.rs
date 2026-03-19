//! `typeway-client` — type-safe HTTP client derived from Wayward API types.
//!
//! The client calls endpoints using the same types as the server. If the
//! server's API type changes, the client fails to compile until updated.

pub mod call;
pub mod client;
pub mod config;
pub mod error;
pub mod retry;

pub use call::CallEndpoint;
pub use client::Client;
pub use config::ClientConfig;
pub use error::ClientError;
pub use retry::RetryPolicy;
