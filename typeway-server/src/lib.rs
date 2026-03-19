//! `typeway-server` — Tower/Hyper server integration for the Typeway web framework.
//!
//! This crate provides the HTTP server layer: handler dispatch, request
//! extraction, response encoding, and the type-safe server builder.

pub mod auth;
#[cfg(feature = "axum-interop")]
pub mod axum_interop;
pub mod body;
pub mod effects;
pub mod error;
pub mod extract;
pub mod handler;
pub mod handler_for;
#[cfg(feature = "multipart")]
pub mod multipart;
pub mod negotiate;
#[cfg(feature = "openapi")]
pub mod openapi;
pub mod production;
pub mod request_id;
pub mod response;
pub mod secure_headers;
pub mod router;
pub mod server;
pub mod serves;
#[cfg(feature = "tls")]
pub mod tls;
pub mod typed;
pub mod typed_bind;
pub mod typed_response;
#[cfg(feature = "ws")]
pub mod typed_ws;
#[cfg(feature = "ws")]
pub mod ws;

pub use body::{body_from_stream, empty_body, sse_body, BoxBody};
pub use error::JsonError;
pub use extract::{
    Cookie, CookieJar, Extension, FromRequest, FromRequestParts, Header, NamedCookie, NamedHeader,
    Path, PathSegments, Query, State,
};
pub use handler::{into_boxed_handler, BoxedHandler, Handler, ResponseFuture};
pub use handler_for::{bind, BindableEndpoint, BoundHandler};
pub use negotiate::{
    AcceptHeader, CsvFormat, HtmlFormat, JsonFormat, NegotiateFormats, NegotiatedResponse, RenderAs,
    RenderAsXml, TextFormat, XmlFormat,
};
pub use response::{IntoResponse, Json};
pub use router::{Router, RouterService};
pub use secure_headers::SecureHeadersLayer;
pub use effects::{EffectfulLayeredServer, EffectfulServer};
pub use server::{serve, LayeredServer, Server};
pub use serves::Serves;

/// Re-export tower-http for middleware layers.
pub use tower_http;

/// Re-export tracing for structured logging.
pub use tracing;
