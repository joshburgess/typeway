//! Unified REST + gRPC serving.
//!
//! When the `grpc` feature is enabled, [`Server::with_grpc`] returns a
//! [`GrpcServer`] that serves both REST and gRPC on the same port. Incoming
//! requests are routed based on the `content-type` header:
//!
//! - `application/grpc*` requests are handled by the gRPC bridge, which
//!   translates them into REST requests and forwards them to the same handlers.
//! - All other requests are handled by the normal REST router.
//!
//! # Example
//!
//! ```ignore
//! Server::<API>::new(handlers)
//!     .with_state(state)
//!     .with_grpc("UserService", "users.v1")
//!     .serve("0.0.0.0:3000".parse()?)
//!     .await?;
//! ```

use std::convert::Infallible;
use std::future::Future;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;

use typeway_core::ApiSpec;
use typeway_grpc::service::{ApiToServiceDescriptor, GrpcServiceDescriptor};
use typeway_grpc::status::http_to_grpc_code;
use typeway_grpc::CollectRpcs;

use crate::body::{empty_body, BoxBody};
use crate::router::{Router, RouterService};

/// A server that serves both REST and gRPC on the same port.
///
/// Created by [`Server::with_grpc`](crate::server::Server::with_grpc).
/// The gRPC bridge translates incoming gRPC calls into REST requests,
/// routing them through the same handler logic.
///
/// # Type parameter
///
/// - `A`: The API type (a tuple of endpoints). Must implement both
///   [`ApiSpec`] and [`CollectRpcs`].
pub struct GrpcServer<A: ApiSpec> {
    router: Arc<Router>,
    service_name: String,
    package: String,
    _api: PhantomData<A>,
}

impl<A: ApiSpec + CollectRpcs> GrpcServer<A> {
    /// Create a new `GrpcServer` wrapping the given router.
    pub(crate) fn new(router: Arc<Router>, service_name: String, package: String) -> Self {
        GrpcServer {
            router,
            service_name,
            package,
            _api: PhantomData,
        }
    }

    /// Add shared application state accessible via
    /// [`State<T>`](crate::extract::State) extractors.
    pub fn with_state<T: Clone + Send + Sync + 'static>(self, state: T) -> Self {
        self.router.set_state_injector(Arc::new(move |ext| {
            ext.insert(state.clone());
        }));
        self
    }

    /// Set a path prefix for all routes.
    pub fn nest(self, prefix: &str) -> Self {
        self.router.set_prefix(prefix);
        self
    }

    /// Set the maximum request body size in bytes.
    pub fn max_body_size(self, max: usize) -> Self {
        self.router.set_max_body_size(max);
        self
    }

    /// Start serving both REST and gRPC.
    ///
    /// Binds to the given address and accepts connections. REST requests are
    /// routed to the typeway router directly; gRPC requests are translated
    /// through the bridge and then routed to the same handlers.
    pub async fn serve(
        self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(addr).await?;
        tracing::info!("Listening on http://{addr} (REST + gRPC)");
        tracing::info!(
            "  gRPC service: {}.{}",
            self.package,
            self.service_name
        );
        self.serve_with_shutdown(listener, std::future::pending())
            .await
    }

    /// Start serving with graceful shutdown.
    ///
    /// Both REST and gRPC are served. When the `shutdown` future completes,
    /// the server stops accepting new connections.
    pub async fn serve_with_shutdown(
        self,
        listener: TcpListener,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let descriptor = Arc::new(A::service_descriptor(&self.service_name, &self.package));

        let multiplexer = Multiplexer {
            rest: RouterService::new(self.router.clone()),
            grpc_descriptor: descriptor,
            router: self.router,
        };

        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                result = listener.accept() => {
                    let (stream, _) = result?;
                    let io = TokioIo::new(stream);
                    let svc = multiplexer.clone();
                    let hyper_svc = hyper_util::service::TowerToHyperService::new(svc);

                    tokio::task::spawn(async move {
                        if let Err(e) = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                            .serve_connection(io, hyper_svc)
                            .await
                        {
                            tracing::debug!("Connection closed: {e}");
                        }
                    });
                }
                () = &mut shutdown => {
                    tracing::info!("Shutting down gracefully...");
                    return Ok(());
                }
            }
        }
    }

    /// Get a reference to the service descriptor that would be used for gRPC.
    ///
    /// Useful for inspecting the generated gRPC method mappings.
    pub fn service_descriptor(&self) -> GrpcServiceDescriptor {
        A::service_descriptor(&self.service_name, &self.package)
    }
}

// ---------------------------------------------------------------------------
// Internal multiplexer
// ---------------------------------------------------------------------------

/// Routes requests to either the REST router or the gRPC bridge based on
/// the `content-type` header.
#[derive(Clone)]
struct Multiplexer {
    rest: RouterService,
    grpc_descriptor: Arc<GrpcServiceDescriptor>,
    router: Arc<Router>,
}

impl tower_service::Service<http::Request<hyper::body::Incoming>> for Multiplexer {
    type Response = http::Response<BoxBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<hyper::body::Incoming>) -> Self::Future {
        if typeway_grpc::is_grpc_request(&req) {
            let descriptor = self.grpc_descriptor.clone();
            let router = self.router.clone();

            Box::pin(async move {
                let grpc_path = req.uri().path().to_string();

                // Find the matching gRPC method.
                let method = descriptor.find_method(&grpc_path);

                let method = match method {
                    Some(m) => m,
                    None => {
                        // UNIMPLEMENTED (12)
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

                if let Ok(uri) = method.rest_path.parse::<http::Uri>() {
                    parts.uri = uri;
                }

                // Remove the gRPC content-type so the REST router handles
                // it normally (e.g., as application/json).
                parts.headers.remove(http::header::CONTENT_TYPE);
                parts
                    .headers
                    .insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/json"),
                    );

                let rest_req = http::Request::from_parts(parts, body);
                let rest_res = router.route(rest_req).await;

                // Translate the HTTP response to a gRPC response.
                let (mut parts, body) = rest_res.into_parts();
                let grpc_code = http_to_grpc_code(parts.status);

                // gRPC always returns HTTP 200; the real status is in grpc-status.
                parts.status = http::StatusCode::OK;
                parts.headers.insert(
                    "grpc-status",
                    grpc_code
                        .as_i32()
                        .to_string()
                        .parse()
                        .expect("valid header value for grpc-status"),
                );
                parts.headers.insert(
                    "content-type",
                    http::HeaderValue::from_static("application/grpc+json"),
                );

                Ok(http::Response::from_parts(parts, body))
            })
        } else {
            // Regular REST request — delegate to the router service.
            let mut rest = self.rest.clone();
            Box::pin(async move { tower_service::Service::call(&mut rest, req).await })
        }
    }
}

/// Helper: create a [`GrpcServer`] from a router and service metadata.
///
/// This is called by [`Server::with_grpc`](crate::server::Server::with_grpc).
pub(crate) fn make_grpc_server<A: ApiSpec + CollectRpcs>(
    router: Arc<Router>,
    service_name: &str,
    package: &str,
) -> GrpcServer<A> {
    GrpcServer::new(router, service_name.to_string(), package.to_string())
}
