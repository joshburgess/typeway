//! `wayward-server` — Tower/Hyper server integration for the Wayward web framework.
//!
//! This crate provides the HTTP server layer: handler dispatch, request
//! extraction, response encoding, and the type-safe server builder.

#[cfg(feature = "axum-interop")]
pub mod axum_interop;
pub mod body;
pub mod extract;
pub mod handler;
pub mod handler_for;
#[cfg(feature = "openapi")]
pub mod openapi;
pub mod response;
pub mod router;
pub mod server;
pub mod serves;

pub use body::BoxBody;
pub use extract::{FromRequest, FromRequestParts, Path, Query, State};
pub use handler::Handler;
pub use handler_for::{bind, BindableEndpoint, BoundHandler};
pub use response::{IntoResponse, Json};
pub use router::{Router, RouterService};
pub use server::{serve, LayeredServer, Server};
pub use serves::Serves;

/// Re-export tower-http for middleware layers.
pub use tower_http;
