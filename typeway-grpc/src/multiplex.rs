//! Content-type based request multiplexer for REST/gRPC routing.
//!
//! [`GrpcMultiplexer`] inspects the `content-type` header of incoming requests
//! and routes them to either a REST service or a gRPC service. Requests with
//! `content-type: application/grpc*` go to the gRPC service; all others go to
//! the REST service.
//!
//! This module provides the building block for unified REST+gRPC serving.
//! The higher-level integration is in `typeway-server`'s `grpc` module.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A multiplexer that routes requests to either a REST service or a gRPC
/// service based on the `content-type` header.
///
/// - Requests with `content-type: application/grpc*` go to the gRPC service
/// - All other requests go to the REST service
///
/// Both services must have the same `Response` and `Error` types.
pub struct GrpcMultiplexer<Rest, Grpc> {
    rest: Rest,
    grpc: Grpc,
}

impl<Rest: Clone, Grpc: Clone> Clone for GrpcMultiplexer<Rest, Grpc> {
    fn clone(&self) -> Self {
        GrpcMultiplexer {
            rest: self.rest.clone(),
            grpc: self.grpc.clone(),
        }
    }
}

impl<Rest, Grpc> GrpcMultiplexer<Rest, Grpc> {
    /// Create a new multiplexer routing between the given REST and gRPC
    /// services.
    pub fn new(rest: Rest, grpc: Grpc) -> Self {
        GrpcMultiplexer { rest, grpc }
    }
}

/// Check whether a request has a gRPC content-type header.
pub fn is_grpc_request<B>(req: &http::Request<B>) -> bool {
    req.headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("application/grpc"))
}

impl<Rest, Grpc, Body, Resp> tower_service::Service<http::Request<Body>>
    for GrpcMultiplexer<Rest, Grpc>
where
    Rest: tower_service::Service<http::Request<Body>, Response = Resp, Error = Infallible>
        + Clone
        + Send
        + 'static,
    Rest::Future: Send + 'static,
    Grpc: tower_service::Service<http::Request<Body>, Response = Resp, Error = Infallible>
        + Clone
        + Send
        + 'static,
    Grpc::Future: Send + 'static,
    Resp: Send + 'static,
    Body: Send + 'static,
{
    type Response = Resp;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Body>) -> Self::Future {
        if is_grpc_request(&req) {
            let mut grpc = self.grpc.clone();
            Box::pin(async move { grpc.call(req).await })
        } else {
            let mut rest = self.rest.clone();
            Box::pin(async move { rest.call(req).await })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_grpc_content_type() {
        let req = http::Request::builder()
            .header(http::header::CONTENT_TYPE, "application/grpc")
            .body(())
            .unwrap();
        assert!(is_grpc_request(&req));
    }

    #[test]
    fn detects_grpc_json_content_type() {
        let req = http::Request::builder()
            .header(http::header::CONTENT_TYPE, "application/grpc+json")
            .body(())
            .unwrap();
        assert!(is_grpc_request(&req));
    }

    #[test]
    fn detects_grpc_proto_content_type() {
        let req = http::Request::builder()
            .header(http::header::CONTENT_TYPE, "application/grpc+proto")
            .body(())
            .unwrap();
        assert!(is_grpc_request(&req));
    }

    #[test]
    fn rest_json_is_not_grpc() {
        let req = http::Request::builder()
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(())
            .unwrap();
        assert!(!is_grpc_request(&req));
    }

    #[test]
    fn no_content_type_is_not_grpc() {
        let req = http::Request::builder().body(()).unwrap();
        assert!(!is_grpc_request(&req));
    }

    #[test]
    fn text_html_is_not_grpc() {
        let req = http::Request::builder()
            .header(http::header::CONTENT_TYPE, "text/html")
            .body(())
            .unwrap();
        assert!(!is_grpc_request(&req));
    }
}
