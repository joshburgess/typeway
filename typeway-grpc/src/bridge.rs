//! gRPC-to-REST bridge via Tower service translation.
//!
//! [`GrpcBridge`] is a Tower service that receives gRPC requests (HTTP/2 with
//! `content-type: application/grpc`) and translates them into REST-style
//! requests that the typeway router can handle. This enables serving both
//! REST and gRPC from the same handler logic.
//!
//! # Simplified JSON bridge
//!
//! This implementation uses JSON-encoded gRPC (`application/grpc+json`)
//! rather than full protobuf encoding. This validates the routing bridge
//! architecture without requiring protobuf serialization. Full protobuf
//! support with length-prefixed framing is planned for Phase D.
//!
//! # How it works
//!
//! 1. Extracts the gRPC method name from the HTTP/2 path
//! 2. Looks up the corresponding REST endpoint via [`GrpcServiceDescriptor`]
//! 3. Rewrites the request URI and method to target the REST endpoint
//! 4. Forwards the request to the inner typeway router
//! 5. Translates the HTTP response status to a gRPC status code

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Empty};

use crate::service::{ApiToServiceDescriptor, GrpcServiceDescriptor};
use crate::status::{http_to_grpc_code, GrpcStatus};

/// The body type used by the bridge.
///
/// Matches the `BoxBody` type from `typeway-server` — an unsync boxed body
/// with a boxed error. Defined locally to avoid a hard dependency on
/// `typeway-server`.
pub type BoxBody = UnsyncBoxBody<Bytes, BoxBodyError>;

/// Error type for the boxed body.
pub type BoxBodyError = Box<dyn std::error::Error + Send + Sync>;

/// Create an empty [`BoxBody`].
fn empty_body() -> BoxBody {
    Empty::new().map_err(|e| match e {}).boxed_unsync()
}

/// A bridge that translates gRPC requests into REST requests
/// and forwards them to a typeway router.
///
/// This enables serving both REST and gRPC from the same handlers.
/// The bridge:
/// 1. Extracts the gRPC method name from the HTTP/2 path
/// 2. Looks up the corresponding REST endpoint
/// 3. Forwards to the typeway router as a regular HTTP request
/// 4. Translates the HTTP response back to gRPC framing
///
/// # Type parameter
///
/// - `S`: The inner REST service (typically `typeway_server::RouterService`).
///   Must implement `tower_service::Service<http::Request<hyper::body::Incoming>>`.
///
/// # Example
///
/// ```ignore
/// use typeway_grpc::bridge::GrpcBridge;
/// use typeway_grpc::service::ApiToServiceDescriptor;
///
/// let descriptor = MyAPI::service_descriptor("UserService", "users.v1");
/// let bridge = GrpcBridge::new(router_service, descriptor);
/// ```
pub struct GrpcBridge<S> {
    /// The inner REST service (typeway router).
    inner: S,
    /// Service descriptor mapping gRPC methods to REST endpoints.
    descriptor: Arc<GrpcServiceDescriptor>,
}

impl<S: Clone> Clone for GrpcBridge<S> {
    fn clone(&self) -> Self {
        GrpcBridge {
            inner: self.inner.clone(),
            descriptor: self.descriptor.clone(),
        }
    }
}

impl<S> GrpcBridge<S> {
    /// Create a new gRPC bridge wrapping the given inner service.
    ///
    /// The `descriptor` maps gRPC method paths to REST endpoints,
    /// enabling the bridge to route incoming gRPC calls to the correct
    /// REST handler.
    pub fn new(inner: S, descriptor: GrpcServiceDescriptor) -> Self {
        GrpcBridge {
            inner,
            descriptor: Arc::new(descriptor),
        }
    }

    /// Create a gRPC bridge from a service and an API type.
    ///
    /// This is a convenience constructor that builds the
    /// [`GrpcServiceDescriptor`] from the API type `A`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let bridge = GrpcBridge::from_api::<MyAPI>(
    ///     router_service,
    ///     "UserService",
    ///     "users.v1",
    /// );
    /// ```
    pub fn from_api<A: ApiToServiceDescriptor>(
        inner: S,
        service_name: &str,
        package: &str,
    ) -> Self {
        let descriptor = A::service_descriptor(service_name, package);
        GrpcBridge::new(inner, descriptor)
    }

    /// Return a reference to the service descriptor.
    pub fn descriptor(&self) -> &GrpcServiceDescriptor {
        &self.descriptor
    }
}

impl<S> tower_service::Service<http::Request<hyper::body::Incoming>> for GrpcBridge<S>
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

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<hyper::body::Incoming>) -> Self::Future {
        let descriptor = self.descriptor.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let grpc_path = req.uri().path().to_string();

            // Find the matching gRPC method.
            let method = descriptor.find_method(&grpc_path);

            let method = match method {
                Some(m) => m,
                None => {
                    // Unimplemented gRPC method — return grpc-status 12 (UNIMPLEMENTED).
                    let mut res = http::Response::new(empty_body());
                    *res.status_mut() = http::StatusCode::OK;
                    res.headers_mut().insert(
                        "grpc-status",
                        http::HeaderValue::from_static("12"),
                    );
                    res.headers_mut().insert(
                        "content-type",
                        http::HeaderValue::from_static("application/grpc"),
                    );
                    return Ok(res);
                }
            };

            // Rewrite the request to target the REST endpoint.
            let (mut parts, body) = req.into_parts();
            parts.method = method.http_method.clone();

            // Parse the REST path as a URI. If it fails, keep the original URI.
            if let Ok(uri) = method.rest_path.parse::<http::Uri>() {
                parts.uri = uri;
            }

            // Forward to the REST router.
            let rest_req = http::Request::from_parts(parts, body);
            let rest_res = inner.call(rest_req).await?;

            // Translate response: map HTTP status to gRPC status code.
            let (mut parts, body) = rest_res.into_parts();
            let grpc_status = GrpcStatus {
                code: http_to_grpc_code(parts.status),
                message: String::new(),
            };

            // gRPC always returns HTTP 200; the real status is in grpc-status
            // and grpc-message headers.
            parts.status = http::StatusCode::OK;
            for (name, value) in grpc_status.to_headers() {
                if let (Ok(name), Ok(value)) = (name.parse::<http::header::HeaderName>(), value.parse::<http::HeaderValue>()) {
                    parts.headers.insert(name, value);
                }
            }
            parts.headers.insert(
                "content-type",
                http::HeaderValue::from_static("application/grpc+json"),
            );

            Ok(http::Response::from_parts(parts, body))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::{GrpcMethodDescriptor, GrpcServiceDescriptor};

    fn test_descriptor() -> GrpcServiceDescriptor {
        GrpcServiceDescriptor {
            name: "TestService".to_string(),
            package: "test.v1".to_string(),
            methods: vec![
                GrpcMethodDescriptor {
                    name: "ListUser".to_string(),
                    full_path: "/test.v1.TestService/ListUser".to_string(),
                    http_method: http::Method::GET,
                    rest_path: "/users".to_string(),
                },
                GrpcMethodDescriptor {
                    name: "GetUser".to_string(),
                    full_path: "/test.v1.TestService/GetUser".to_string(),
                    http_method: http::Method::GET,
                    rest_path: "/users/{}".to_string(),
                },
                GrpcMethodDescriptor {
                    name: "CreateUser".to_string(),
                    full_path: "/test.v1.TestService/CreateUser".to_string(),
                    http_method: http::Method::POST,
                    rest_path: "/users".to_string(),
                },
            ],
        }
    }

    /// Verify GrpcBridge can be constructed (compile test).
    #[test]
    fn bridge_construction() {
        // We can't easily construct a real RouterService here, but we can
        // verify the type constructors work with a mock descriptor.
        let desc = test_descriptor();
        assert_eq!(desc.name, "TestService");
        assert_eq!(desc.methods.len(), 3);

        // Verify find_method works on the descriptor the bridge would use.
        assert!(desc.find_method("/test.v1.TestService/ListUser").is_some());
        assert!(desc.find_method("/test.v1.TestService/GetUser").is_some());
        assert!(desc
            .find_method("/test.v1.TestService/CreateUser")
            .is_some());
        assert!(desc
            .find_method("/test.v1.TestService/DeleteUser")
            .is_none());
    }

    /// Verify that the GrpcBridge type can be cloned when S is Clone.
    #[test]
    fn bridge_is_cloneable() {
        // Use a simple Clone type as the inner service stand-in.
        #[derive(Clone)]
        struct FakeService;

        let desc = test_descriptor();
        let bridge = GrpcBridge::new(FakeService, desc);
        let _bridge2 = bridge.clone();
    }

    /// Verify from_api constructor compiles (requires a CollectRpcs type).
    #[test]
    fn from_api_constructor() {
        #[derive(Clone)]
        struct FakeService;

        // Use a unit-like type that implements CollectRpcs trivially.
        // We need an actual endpoint type here, so we just test with
        // the descriptor directly.
        let desc = GrpcServiceDescriptor {
            name: "Svc".to_string(),
            package: "pkg.v1".to_string(),
            methods: vec![],
        };
        let bridge = GrpcBridge::new(FakeService, desc);
        assert_eq!(bridge.descriptor().name, "Svc");
        assert_eq!(bridge.descriptor().package, "pkg.v1");
    }
}
