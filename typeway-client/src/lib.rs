//! `typeway-client` — type-safe HTTP client derived from Wayward API types.
//!
//! The client calls endpoints using the same types as the server. If the
//! server's API type changes, the client fails to compile until updated.

pub mod call;
pub mod client;
pub mod error;

pub use call::CallEndpoint;
pub use client::Client;
pub use error::ClientError;
