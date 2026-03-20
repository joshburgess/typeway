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
//! Built-in gRPC services (reflection and health check) are handled directly
//! by the multiplexer without going through the REST bridge.
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
use typeway_grpc::health::HealthService;
use typeway_grpc::reflection::ReflectionService;
use typeway_grpc::service::{ApiToServiceDescriptor, GrpcServiceDescriptor};
use typeway_grpc::status::http_to_grpc_code;
use typeway_grpc::CollectRpcs;

use crate::body::{body_from_bytes, empty_body, BoxBody};
use crate::router::{Router, RouterService};

/// A server that serves both REST and gRPC on the same port.
///
/// Created by [`Server::with_grpc`](crate::server::Server::with_grpc).
/// The gRPC bridge translates incoming gRPC calls into REST requests,
/// routing them through the same handler logic.
///
/// Includes built-in support for:
/// - **Server reflection** (`grpc.reflection.v1alpha`) — enabled by default,
///   allows tools like `grpcurl` to discover available services.
/// - **Health checking** (`grpc.health.v1.Health/Check`) — always enabled,
///   with a runtime-toggleable serving status for graceful shutdown.
///
/// # Type parameter
///
/// - `A`: The API type (a tuple of endpoints). Must implement both
///   [`ApiSpec`] and [`CollectRpcs`].
pub struct GrpcServer<A: ApiSpec> {
    router: Arc<Router>,
    service_name: String,
    package: String,
    reflection: ReflectionService,
    health: HealthService,
    reflection_enabled: bool,
    _api: PhantomData<A>,
}

impl<A: ApiSpec + CollectRpcs> GrpcServer<A> {
    /// Create a new `GrpcServer` wrapping the given router.
    pub(crate) fn new(router: Arc<Router>, service_name: String, package: String) -> Self {
        let reflection = ReflectionService::from_api::<A>(&service_name, &package);
        let health = HealthService::new();
        GrpcServer {
            router,
            service_name,
            package,
            reflection,
            health,
            reflection_enabled: true,
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

    /// Enable or disable gRPC server reflection.
    ///
    /// Reflection is enabled by default. When enabled, gRPC clients can
    /// discover available services at runtime (e.g., `grpcurl -plaintext
    /// localhost:3000 list`).
    pub fn with_reflection(mut self, enabled: bool) -> Self {
        self.reflection_enabled = enabled;
        self
    }

    /// Get a handle to the health service.
    ///
    /// Use this to toggle the serving status during graceful shutdown:
    ///
    /// ```ignore
    /// let grpc = server.with_grpc("Svc", "pkg.v1");
    /// let health = grpc.health_service();
    ///
    /// // In a shutdown hook:
    /// health.set_not_serving();
    /// ```
    pub fn health_service(&self) -> HealthService {
        self.health.clone()
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
        if self.reflection_enabled {
            tracing::info!("  gRPC reflection: enabled");
        }
        tracing::info!("  gRPC health check: enabled");
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
            reflection: Arc::new(self.reflection),
            health: self.health,
            reflection_enabled: self.reflection_enabled,
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

    /// Apply a Tower middleware layer.
    ///
    /// The layer wraps the entire multiplexer service (both REST and gRPC).
    /// This is the equivalent of [`Server::layer`](crate::server::Server::layer)
    /// for the gRPC server.
    ///
    /// # Example
    ///
    /// ```ignore
    /// server
    ///     .with_grpc("Svc", "pkg.v1")
    ///     .layer(CorsLayer::permissive())
    ///     .serve(addr)
    ///     .await?;
    /// ```
    pub fn layer<L>(self, layer: L) -> LayeredGrpcServer<A, L::Service>
    where
        L: tower_layer::Layer<Multiplexer>,
        L::Service: tower_service::Service<
                http::Request<hyper::body::Incoming>,
                Response = http::Response<BoxBody>,
                Error = Infallible,
            > + Clone
            + Send
            + 'static,
        <L::Service as tower_service::Service<http::Request<hyper::body::Incoming>>>::Future:
            Send + 'static,
    {
        let descriptor = Arc::new(A::service_descriptor(&self.service_name, &self.package));
        let multiplexer = Multiplexer {
            rest: RouterService::new(self.router.clone()),
            grpc_descriptor: descriptor,
            router: self.router,
            reflection: Arc::new(self.reflection),
            health: self.health,
            reflection_enabled: self.reflection_enabled,
        };
        LayeredGrpcServer {
            service: layer.layer(multiplexer),
            _api: PhantomData,
        }
    }
}

/// A gRPC+REST server with Tower middleware layers applied.
///
/// Created by [`GrpcServer::layer`]. Supports `.serve()` for starting the server.
pub struct LayeredGrpcServer<A: ApiSpec, S> {
    service: S,
    _api: PhantomData<A>,
}

impl<A, S> LayeredGrpcServer<A, S>
where
    A: ApiSpec + CollectRpcs,
    S: tower_service::Service<
            http::Request<hyper::body::Incoming>,
            Response = http::Response<BoxBody>,
            Error = Infallible,
        > + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    /// Start serving both REST and gRPC.
    pub async fn serve(
        self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(addr).await?;
        tracing::info!("Listening on http://{addr} (REST + gRPC, layered)");
        self.serve_with_shutdown(listener, std::future::pending())
            .await
    }

    /// Start serving with graceful shutdown.
    pub async fn serve_with_shutdown(
        self,
        listener: TcpListener,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let svc = self.service;
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                result = listener.accept() => {
                    let (stream, _) = result?;
                    let io = TokioIo::new(stream);
                    let svc = svc.clone();
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

    /// Apply another Tower middleware layer.
    pub fn layer<L>(self, layer: L) -> LayeredGrpcServer<A, L::Service>
    where
        L: tower_layer::Layer<S>,
        L::Service: tower_service::Service<
                http::Request<hyper::body::Incoming>,
                Response = http::Response<BoxBody>,
                Error = Infallible,
            > + Clone
            + Send
            + 'static,
        <L::Service as tower_service::Service<http::Request<hyper::body::Incoming>>>::Future:
            Send + 'static,
    {
        LayeredGrpcServer {
            service: layer.layer(self.service),
            _api: PhantomData,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal multiplexer
// ---------------------------------------------------------------------------

/// Routes requests to either the REST router or the gRPC bridge based on
/// the `content-type` header.
///
/// For gRPC requests, built-in services (reflection and health check) are
/// handled directly before falling through to the REST bridge for
/// application-defined methods.
///
/// This type is exposed so that Tower layers can be applied over the
/// combined REST + gRPC service via [`GrpcServer::layer`].
#[derive(Clone)]
pub struct Multiplexer {
    pub(crate) rest: RouterService,
    pub(crate) grpc_descriptor: Arc<GrpcServiceDescriptor>,
    pub(crate) router: Arc<Router>,
    pub(crate) reflection: Arc<ReflectionService>,
    pub(crate) health: HealthService,
    pub(crate) reflection_enabled: bool,
}

/// Build a gRPC JSON response with the given body and status code 0 (OK).
///
/// The response body is wrapped in gRPC length-prefix framing.
fn grpc_json_response(json_body: &str) -> http::Response<BoxBody> {
    let framed = typeway_grpc::framing::encode_grpc_frame(json_body.as_bytes());
    let mut res = http::Response::new(body_from_bytes(bytes::Bytes::from(framed)));
    *res.status_mut() = http::StatusCode::OK;
    res.headers_mut().insert(
        "grpc-status",
        http::HeaderValue::from_static("0"),
    );
    res.headers_mut().insert(
        "content-type",
        http::HeaderValue::from_static("application/grpc+json"),
    );
    res
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
            let reflection = self.reflection.clone();
            let health = self.health.clone();
            let reflection_enabled = self.reflection_enabled;

            Box::pin(async move {
                let grpc_path = req.uri().path().to_string();

                // Handle built-in gRPC services before method lookup.

                // 1. Health check service.
                if HealthService::is_health_path(&grpc_path) {
                    let response_json = health.handle_request();
                    return Ok(grpc_json_response(&response_json));
                }

                // 2. Server reflection service.
                if reflection_enabled && ReflectionService::is_reflection_path(&grpc_path) {
                    // Read the request body to determine the query type.
                    let (parts, body) = req.into_parts();
                    let body_bytes = match http_body_util::BodyExt::collect(body).await {
                        Ok(collected) => collected.to_bytes(),
                        Err(_) => bytes::Bytes::new(),
                    };
                    // Strip gRPC framing if present.
                    let unframed = typeway_grpc::framing::decode_grpc_frame(&body_bytes)
                        .unwrap_or(&body_bytes);
                    let body_str = String::from_utf8_lossy(unframed);
                    let _ = parts; // consumed
                    let response_json = reflection.handle_request(&body_str);
                    return Ok(grpc_json_response(&response_json));
                }

                // 3. Application-defined gRPC methods (REST bridge).
                let method = descriptor.find_method(&grpc_path);

                let method = match method {
                    Some(m) => m,
                    None => {
                        // UNIMPLEMENTED (12) with a descriptive grpc-message.
                        let status = typeway_grpc::GrpcStatus::unimplemented(
                            &format!("method '{}' not found in service", grpc_path),
                        );
                        let mut res = http::Response::new(empty_body());
                        *res.status_mut() = http::StatusCode::OK;
                        for (name, value) in status.to_headers() {
                            if let (Ok(name), Ok(value)) = (
                                name.parse::<http::header::HeaderName>(),
                                value.parse::<http::HeaderValue>(),
                            ) {
                                res.headers_mut().insert(name, value);
                            }
                        }
                        res.headers_mut().insert(
                            "content-type",
                            http::HeaderValue::from_static("application/grpc"),
                        );
                        return Ok(res);
                    }
                };

                // Parse the grpc-timeout header for deadline propagation.
                let grpc_timeout = req
                    .headers()
                    .get("grpc-timeout")
                    .and_then(|v| v.to_str().ok())
                    .and_then(typeway_grpc::parse_grpc_timeout);

                // Collect the body and strip gRPC framing.
                let (mut parts, body) = req.into_parts();
                let body_bytes = match http_body_util::BodyExt::collect(body).await {
                    Ok(collected) => collected.to_bytes(),
                    Err(_) => bytes::Bytes::new(),
                };

                // Strip gRPC length-prefix framing if present.
                let unframed = typeway_grpc::framing::decode_grpc_frame(&body_bytes)
                    .map(bytes::Bytes::copy_from_slice)
                    .unwrap_or(body_bytes);

                // Rewrite the request to target the REST endpoint.
                parts.method = method.http_method.clone();

                if let Ok(uri) = method.rest_path.parse::<http::Uri>() {
                    parts.uri = uri;
                }

                // Set content-type to JSON for the REST handler.
                parts.headers.remove(http::header::CONTENT_TYPE);
                parts
                    .headers
                    .insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/json"),
                    );

                // Route with pre-collected bytes, applying timeout if present.
                let rest_res = if let Some(timeout_duration) = grpc_timeout {
                    match tokio::time::timeout(
                        timeout_duration,
                        router.route_with_bytes(parts, unframed),
                    )
                    .await
                    {
                        Ok(res) => res,
                        Err(_) => {
                            // Deadline exceeded — return grpc-status 4.
                            let grpc_status = typeway_grpc::GrpcStatus {
                                code: typeway_grpc::GrpcCode::DeadlineExceeded,
                                message: "deadline exceeded".to_string(),
                            };
                            let mut res = http::Response::new(empty_body());
                            *res.status_mut() = http::StatusCode::OK;
                            for (name, value) in grpc_status.to_headers() {
                                if let (Ok(name), Ok(value)) = (
                                    name.parse::<http::header::HeaderName>(),
                                    value.parse::<http::HeaderValue>(),
                                ) {
                                    res.headers_mut().insert(name, value);
                                }
                            }
                            res.headers_mut().insert(
                                "content-type",
                                http::HeaderValue::from_static("application/grpc+json"),
                            );
                            return Ok(res);
                        }
                    }
                } else {
                    router.route_with_bytes(parts, unframed).await
                };

                // Collect the REST response body and wrap it in a gRPC frame.
                let (mut res_parts, res_body) = rest_res.into_parts();
                let grpc_code = http_to_grpc_code(res_parts.status);

                let res_bytes = match http_body_util::BodyExt::collect(res_body).await {
                    Ok(collected) => collected.to_bytes(),
                    Err(_) => bytes::Bytes::new(),
                };
                let framed = typeway_grpc::framing::encode_grpc_frame(&res_bytes);

                // gRPC always returns HTTP 200; the real status is in grpc-status.
                res_parts.status = http::StatusCode::OK;
                res_parts.headers.insert(
                    "grpc-status",
                    grpc_code
                        .as_i32()
                        .to_string()
                        .parse()
                        .expect("valid header value for grpc-status"),
                );
                res_parts.headers.insert(
                    "content-type",
                    http::HeaderValue::from_static("application/grpc+json"),
                );

                let framed_body = body_from_bytes(bytes::Bytes::from(framed));
                Ok(http::Response::from_parts(res_parts, framed_body))
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

// ---------------------------------------------------------------------------
// EndpointToRpc / CollectRpcs delegation for wrapper types
// ---------------------------------------------------------------------------

use typeway_grpc::{EndpointToRpc, RpcMethod};

/// `Protected<Auth, E>` delegates gRPC mapping to the inner endpoint.
impl<Auth, E: EndpointToRpc> EndpointToRpc for crate::auth::Protected<Auth, E> {
    fn to_rpc() -> RpcMethod {
        E::to_rpc()
    }
}

/// `Validated<V, E>` delegates gRPC mapping to the inner endpoint.
impl<V: Send + Sync + 'static, E: EndpointToRpc> EndpointToRpc for crate::typed::Validated<V, E> {
    fn to_rpc() -> RpcMethod {
        E::to_rpc()
    }
}
