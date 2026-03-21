//! Unified REST + gRPC serving.
//!
//! When the `grpc` feature is enabled, [`Server::with_grpc`] returns a
//! [`GrpcServer`] that serves both REST and gRPC on the same port. Incoming
//! requests are routed based on the `content-type` header:
//!
//! - `application/grpc*` requests are dispatched directly to handlers via
//!   the native gRPC dispatch (HashMap lookup, real HTTP/2 trailers).
//! - All other requests are handled by the normal REST router.
//!
//! Built-in gRPC services (reflection and health check) are handled directly
//! by the multiplexer.
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
use std::sync::Arc;

use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;

use typeway_core::ApiSpec;
use typeway_grpc::health::HealthService;
use typeway_grpc::reflection::ReflectionService;
use typeway_grpc::service::{ApiToServiceDescriptor, GrpcServiceDescriptor};
use typeway_grpc::CollectRpcs;

use crate::body::BoxBody;
use crate::router::{Router, RouterService};

/// A server that serves both REST and gRPC on the same port.
///
/// Created by [`Server::with_grpc`](crate::server::Server::with_grpc).
/// gRPC requests are dispatched directly to handlers via HashMap lookup
/// with real HTTP/2 trailers. REST requests go through the normal router.
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
    grpc_spec_json: Option<Arc<String>>,
    grpc_docs_html: Option<Arc<String>>,
    #[cfg(feature = "grpc-proto-binary")]
    transcoder: Option<Arc<typeway_grpc::ProtoTranscoder>>,
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
            grpc_spec_json: None,
            grpc_docs_html: None,
            #[cfg(feature = "grpc-proto-binary")]
            transcoder: None,
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
    pub fn with_reflection(mut self, enabled: bool) -> Self {
        self.reflection_enabled = enabled;
        self
    }

    /// Get a handle to the health service.
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

    /// Serve a gRPC service specification at `GET /grpc-spec` (JSON) and an
    /// HTML documentation page at `GET /grpc-docs`.
    pub fn with_grpc_docs(mut self) -> Self {
        use typeway_grpc::spec::ApiToGrpcSpec;
        let spec = A::grpc_spec(&self.service_name, &self.package);
        let json = serde_json::to_string_pretty(&spec).expect("spec serialization");
        let html = typeway_grpc::docs_page::generate_docs_html(&spec);
        self.grpc_spec_json = Some(Arc::new(json));
        self.grpc_docs_html = Some(Arc::new(html));
        self
    }

    /// Serve a gRPC service specification with handler documentation applied.
    pub fn with_grpc_docs_with_handler_docs(mut self, docs: &[typeway_core::HandlerDoc]) -> Self {
        use typeway_grpc::spec::ApiToGrpcSpec;
        let spec = A::grpc_spec_with_docs(&self.service_name, &self.package, docs);
        let json = serde_json::to_string_pretty(&spec).expect("spec serialization");
        let html = typeway_grpc::docs_page::generate_docs_html(&spec);
        self.grpc_spec_json = Some(Arc::new(json));
        self.grpc_docs_html = Some(Arc::new(html));
        self
    }

    /// Enable binary protobuf support for standard gRPC client interop.
    #[cfg(feature = "grpc-proto-binary")]
    pub fn with_proto_binary(mut self) -> Self {
        use typeway_grpc::spec::ApiToGrpcSpec;
        let spec = A::grpc_spec(&self.service_name, &self.package);
        self.transcoder = Some(Arc::new(typeway_grpc::ProtoTranscoder::new(spec)));
        self
    }

    /// Start serving both REST and gRPC.
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
    pub async fn serve_with_shutdown(
        self,
        listener: TcpListener,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let multiplexer = self.build_multiplexer();

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

    /// Get a reference to the service descriptor.
    pub fn service_descriptor(&self) -> GrpcServiceDescriptor {
        A::service_descriptor(&self.service_name, &self.package)
    }

    /// Apply a Tower middleware layer.
    pub fn layer<L>(self, layer: L) -> LayeredGrpcServer<A, L::Service>
    where
        L: tower_layer::Layer<crate::grpc_native::GrpcMultiplexer>,
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
        let multiplexer = self.build_multiplexer();
        LayeredGrpcServer {
            service: layer.layer(multiplexer),
            _api: PhantomData,
        }
    }

    /// Build the native multiplexer from the current configuration.
    fn build_multiplexer(self) -> crate::grpc_native::GrpcMultiplexer {
        let descriptor = A::service_descriptor(&self.service_name, &self.package);
        let grpc_router = crate::grpc_native::GrpcRouter::from_router(
            &self.router,
            &descriptor,
        );

        crate::grpc_native::GrpcMultiplexer {
            rest: RouterService::new(self.router),
            grpc_router: Arc::new(grpc_router),
            reflection: Arc::new(self.reflection),
            health: self.health,
            reflection_enabled: self.reflection_enabled,
            grpc_spec_json: self.grpc_spec_json,
            grpc_docs_html: self.grpc_docs_html,
            #[cfg(feature = "grpc-proto-binary")]
            transcoder: self.transcoder,
        }
    }
}

/// A gRPC+REST server with Tower middleware layers applied.
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

/// Helper: create a [`GrpcServer`] from a router and service metadata.
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

// ---------------------------------------------------------------------------
// GrpcReady delegation for server-specific wrapper types
// ---------------------------------------------------------------------------

impl<Auth, E: typeway_grpc::GrpcReady> typeway_grpc::GrpcReady for crate::auth::Protected<Auth, E> {}

impl<V: Send + Sync + 'static, E: typeway_grpc::GrpcReady> typeway_grpc::GrpcReady
    for crate::typed::Validated<V, E>
{
}

// ---------------------------------------------------------------------------
// BindableEndpoint delegation for streaming wrapper types
// ---------------------------------------------------------------------------

use crate::handler_for::BindableEndpoint;

impl<E: BindableEndpoint> BindableEndpoint for typeway_grpc::streaming::ServerStream<E> {
    fn method() -> http::Method {
        E::method()
    }
    fn pattern() -> String {
        E::pattern()
    }
    fn match_fn() -> crate::router::MatchFn {
        E::match_fn()
    }
}

impl<E: BindableEndpoint> BindableEndpoint for typeway_grpc::streaming::ClientStream<E> {
    fn method() -> http::Method {
        E::method()
    }
    fn pattern() -> String {
        E::pattern()
    }
    fn match_fn() -> crate::router::MatchFn {
        E::match_fn()
    }
}

impl<E: BindableEndpoint> BindableEndpoint for typeway_grpc::streaming::BidirectionalStream<E> {
    fn method() -> http::Method {
        E::method()
    }
    fn pattern() -> String {
        E::pattern()
    }
    fn match_fn() -> crate::router::MatchFn {
        E::match_fn()
    }
}
