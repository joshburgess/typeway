//! Bind functions for typed endpoint wrappers.
//!
//! These create `BoundHandler`s that add runtime enforcement (validation,
//! content-type checking) on top of the type-level declarations.

use serde::de::DeserializeOwned;

use crate::handler::{into_boxed_handler, BoxedHandler, Handler};
use crate::handler_for::{BindableEndpoint, BoundHandler};
use crate::response::IntoResponse;
use crate::typed::*;

// ===========================================================================
// bind_validated — validates body before handler
// ===========================================================================

/// Bind a handler to a `Validated<V, E>` endpoint.
///
/// Deserializes the request body, runs the validator, and only calls the
/// handler if validation passes. Returns 422 on validation failure.
pub fn bind_validated<V, E, H, Args>(handler: H) -> BoundHandler<Validated<V, E>>
where
    V: Validate<E::Req>,
    E: BindableEndpoint + HasReqType,
    E::Req: DeserializeOwned + Send + 'static,
    H: Handler<Args>,
    Args: 'static,
{
    let method = E::method();
    let pattern = E::pattern();
    let match_fn = E::match_fn();

    // Wrap the handler with validation.
    let inner = into_boxed_handler(handler);
    let boxed: BoxedHandler = std::sync::Arc::new(move |parts, body| {
        // Try to deserialize and validate the body.
        let validation_result: Result<(), String> = serde_json::from_slice::<E::Req>(&body)
            .map_err(|e| format!("invalid request body: {e}"))
            .and_then(|parsed| V::validate(&parsed));

        match validation_result {
            Ok(()) => inner(parts, body),
            Err(msg) => {
                let error = crate::error::JsonError::unprocessable(msg);
                Box::pin(async move { error.into_response() })
            }
        }
    });

    BoundHandler::new(method, pattern, match_fn, boxed)
}

/// Helper trait to extract the Req type from an endpoint.
pub trait HasReqType {
    type Req;
}

impl<M: typeway_core::HttpMethod, P: typeway_core::PathSpec, Req, Res, Q, Err> HasReqType
    for typeway_core::Endpoint<M, P, Req, Res, Q, Err>
{
    type Req = Req;
}

// ===========================================================================
// bind_content_type — checks Content-Type before handler
// ===========================================================================

/// Bind a handler to a `ContentType<C, E>` endpoint.
///
/// Checks the Content-Type header before calling the handler.
/// Returns 415 Unsupported Media Type if the header doesn't match.
pub fn bind_content_type<C, E, H, Args>(handler: H) -> BoundHandler<ContentType<C, E>>
where
    C: ContentTypeMarker,
    E: BindableEndpoint,
    H: Handler<Args>,
    Args: 'static,
{
    let method = E::method();
    let pattern = E::pattern();
    let match_fn = E::match_fn();

    let inner = into_boxed_handler(handler);
    let expected = C::CONTENT_TYPE;
    let boxed: BoxedHandler = std::sync::Arc::new(move |parts: http::request::Parts, body: bytes::Bytes| {
        let ct = parts
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !ct.starts_with(expected) {
            let error = crate::error::JsonError::new(
                http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
                format!("expected Content-Type: {expected}, got: {ct}"),
            );
            return Box::pin(async move { error.into_response() });
        }

        inner(parts, body)
    });

    BoundHandler::new(method, pattern, match_fn, boxed)
}

// ===========================================================================
// Convenience macros
// ===========================================================================

/// Bind a handler with body validation.
#[macro_export]
macro_rules! bind_validated {
    ($handler:expr) => {
        $crate::typed_bind::bind_validated::<_, _, _, _>($handler)
    };
}

/// Bind a handler with content-type enforcement.
#[macro_export]
macro_rules! bind_content_type {
    ($handler:expr) => {
        $crate::typed_bind::bind_content_type::<_, _, _, _>($handler)
    };
}
