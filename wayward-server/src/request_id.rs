//! Request ID middleware — assigns a unique ID to each request.
//!
//! The ID is available via the [`RequestId`] extractor and is also
//! set as the `x-request-id` response header.
//!
//! # Example
//!
//! ```ignore
//! use wayward_server::request_id::{RequestId, RequestIdLayer};
//!
//! Server::<API>::new(handlers)
//!     .layer(RequestIdLayer::new())
//!     .serve(addr)
//!     .await?;
//!
//! // In handlers:
//! async fn handler(Extension(id): Extension<RequestId>) -> String {
//!     format!("request: {}", id.0)
//! }
//! ```

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::body::BoxBody;

/// A unique request identifier.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Tower layer that assigns a unique `x-request-id` to each request.
///
/// - Generates a UUID v4 if no `x-request-id` header is present
/// - Preserves an existing `x-request-id` header if present
/// - Injects [`RequestId`] into request extensions (accessible via `Extension<RequestId>`)
/// - Copies the ID to the response `x-request-id` header
#[derive(Clone)]
pub struct RequestIdLayer;

impl RequestIdLayer {
    pub fn new() -> Self {
        RequestIdLayer
    }
}

impl Default for RequestIdLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> tower_layer::Layer<S> for RequestIdLayer {
    type Service = RequestIdService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestIdService { inner }
    }
}

/// The service produced by [`RequestIdLayer`].
#[derive(Clone)]
pub struct RequestIdService<S> {
    inner: S,
}

impl<S> tower_service::Service<http::Request<hyper::body::Incoming>> for RequestIdService<S>
where
    S: tower_service::Service<
            http::Request<hyper::body::Incoming>,
            Response = http::Response<BoxBody>,
            Error = Infallible,
        > + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = http::Response<BoxBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: http::Request<hyper::body::Incoming>) -> Self::Future {
        // Extract or generate request ID.
        let id = req
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Inject into extensions for handler access.
        req.extensions_mut().insert(RequestId(id.clone()));

        let mut inner = self.inner.clone();
        Box::pin(async move {
            let mut resp = inner.call(req).await?;
            // Set response header.
            if let Ok(val) = http::HeaderValue::from_str(&id) {
                resp.headers_mut().insert("x-request-id", val);
            }
            Ok(resp)
        })
    }
}
