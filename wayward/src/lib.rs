//! # Wayward — Type-Level Web Framework for Rust
//!
//! Wayward is a web framework where your entire API is described as a Rust type.
//! Servers, clients, and OpenAPI schemas are all derived from that single type
//! definition. If the types compile, the pieces fit together.
//!
//! Built on Tokio, Tower, and Hyper — fully compatible with the Axum ecosystem.
//!
//! ## Quick Start
//!
//! ```no_run
//! use wayward::prelude::*;
//!
//! // 1. Define path types
//! wayward_path!(type HelloPath = "hello");
//! wayward_path!(type GreetPath = "greet" / String);
//!
//! // 2. Define the API as a type
//! type API = (
//!     GetEndpoint<HelloPath, String>,
//!     GetEndpoint<GreetPath, String>,
//! );
//!
//! // 3. Write handlers
//! async fn hello() -> &'static str { "Hello, world!" }
//! async fn greet(path: Path<GreetPath>) -> String {
//!     let (name,) = path.0;
//!     format!("Hello, {name}!")
//! }
//!
//! // 4. Serve — the compiler verifies every endpoint has a handler
//! #[tokio::main]
//! async fn main() {
//!     Server::<API>::new((
//!         bind::<_, _, _>(hello),
//!         bind::<_, _, _>(greet),
//!     ))
//!     .serve("0.0.0.0:3000".parse().unwrap())
//!     .await
//!     .unwrap();
//! }
//! ```
//!
//! ## Feature Flags
//!
//! | Flag | Default | Description |
//! |------|---------|-------------|
//! | `server` | yes | HTTP server (Tower/Hyper) |
//! | `client` | no | Type-safe HTTP client (reqwest) |
//! | `openapi` | no | OpenAPI 3.1 spec generation |
//! | `full` | no | All features |

// --- Core (always available) ---
pub use wayward_core::*;
pub use wayward_macros::*;

// --- Re-export common deps ---
pub use http;
pub use serde;
pub use serde_json;

// --- Server (feature = "server") ---
#[cfg(feature = "server")]
pub use wayward_server::{
    bind, body_from_stream, empty_body, serve, sse_body, BoundHandler, Extension, FromRequest,
    FromRequestParts, Handler, Header, IntoResponse, Json, JsonError, LayeredServer, NamedHeader,
    Path, Query, Router, RouterService, Server, Serves, State,
};

/// Re-export tower-http for middleware (available when `server` feature is on).
#[cfg(feature = "server")]
pub use wayward_server::tower_http;

// --- Client (feature = "client") ---
#[cfg(feature = "client")]
pub use wayward_client::{CallEndpoint, Client, ClientError};

// --- OpenAPI (feature = "openapi") ---
#[cfg(feature = "openapi")]
pub use wayward_openapi::{ApiToSpec, EndpointToOperation, OpenApiSpec, ToSchema};

/// Convenience prelude — import everything you typically need.
pub mod prelude {
    pub use wayward_core::*;
    pub use wayward_macros::*;

    pub use serde::{Deserialize, Serialize};

    #[cfg(feature = "server")]
    pub use wayward_server::{
        bind, body_from_stream, empty_body, serve, sse_body, tower_http, BoundHandler, Extension,
        FromRequest, FromRequestParts, Handler, Header, IntoResponse, Json, JsonError,
        LayeredServer, NamedHeader, Path, Query, Router, RouterService, Server, Serves, State,
    };

    #[cfg(feature = "client")]
    pub use wayward_client::{CallEndpoint, Client, ClientError};

    #[cfg(feature = "openapi")]
    pub use wayward_openapi::{ApiToSpec, EndpointToOperation, OpenApiSpec, ToSchema};
}
