//! Server-side middleware effects tracking.
//!
//! [`EffectfulServer`] wraps a [`Server`](crate::server::Server) and tracks which middleware effects
//! have been provided via `.provide::<E>()`. The `.serve()` method only
//! compiles when all effects declared in the API type (via
//! [`Requires<E, _>`](typeway_core::effects::Requires)) have been discharged.
//!
//! # Example
//!
//! ```ignore
//! use typeway_core::effects::*;
//!
//! type API = (
//!     Requires<AuthRequired, GetEndpoint<UserPath, User>>,
//!     Requires<CorsRequired, GetEndpoint<PublicPath, Data>>,
//!     GetEndpoint<HealthPath, String>,
//! );
//!
//! EffectfulServer::<API>::new(handlers)
//!     .provide::<AuthRequired>()
//!     .layer(auth_layer)
//!     .provide::<CorsRequired>()
//!     .layer(CorsLayer::permissive())
//!     .serve(addr)   // only compiles because both effects are provided
//!     .await;
//! ```

use std::convert::Infallible;
use std::future::Future;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;

use typeway_core::effects::{AllProvided, ECons, ENil, Effect};
use typeway_core::ApiSpec;

use crate::body::BoxBody;
use crate::router::{Router, RouterService};
use crate::server::LayeredServer;
use crate::serves::Serves;

/// A server builder that tracks which middleware effects have been provided.
///
/// The `Provided` type parameter is a type-level list of effects that have
/// been discharged via `.provide::<E>()`. It starts as [`ENil`] and grows
/// with each `.provide()` call.
///
/// The `.serve()` method requires `A: AllProvided<Provided, _>`, ensuring
/// that every [`Requires<E, _>`](typeway_core::effects::Requires) in the
/// API type has a corresponding `.provide::<E>()`.
pub struct EffectfulServer<A: ApiSpec, Provided = ENil> {
    router: Arc<Router>,
    _api: PhantomData<A>,
    _provided: PhantomData<Provided>,
}

impl<A: ApiSpec> EffectfulServer<A, ENil> {
    /// Create an effectful server from a handler tuple.
    ///
    /// The handler tuple must cover every endpoint in the API, just like
    /// [`Server::new`](crate::server::Server::new).
    pub fn new<H: Serves<A>>(handlers: H) -> Self {
        let mut router = Router::new();
        handlers.register(&mut router);
        EffectfulServer {
            router: Arc::new(router),
            _api: PhantomData,
            _provided: PhantomData,
        }
    }
}

impl<A: ApiSpec, P> EffectfulServer<A, P> {
    /// Declare that a middleware effect has been provided.
    ///
    /// Each `.provide::<E>()` call adds `E` to the type-level list of
    /// provided effects. Pair this with a `.layer()` call that applies
    /// the actual middleware.
    ///
    /// # Example
    ///
    /// ```ignore
    /// server
    ///     .provide::<AuthRequired>()
    ///     .layer(auth_layer)
    /// ```
    pub fn provide<E: Effect>(self) -> EffectfulServer<A, ECons<E, P>> {
        EffectfulServer {
            router: self.router,
            _api: PhantomData,
            _provided: PhantomData,
        }
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

    /// Add shared state accessible via [`State<T>`](crate::extract::State) extractors.
    pub fn with_state<T: Clone + Send + Sync + 'static>(self, state: T) -> Self {
        self.router.set_state_injector(Arc::new(move |ext| {
            ext.insert(state.clone());
        }));
        self
    }

    /// Apply a Tower middleware layer to the server.
    ///
    /// The layer wraps the entire router service. This is typically paired
    /// with a `.provide::<E>()` call to discharge an effect requirement.
    ///
    /// # Example
    ///
    /// ```ignore
    /// server
    ///     .provide::<CorsRequired>()
    ///     .layer(CorsLayer::permissive())
    /// ```
    pub fn layer<L>(self, layer: L) -> EffectfulLayeredServer<A, P, L::Service>
    where
        L: tower_layer::Layer<RouterService>,
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
        let router = self.router.clone();
        let svc = RouterService::new(self.router);
        let layered = layer.layer(svc);
        EffectfulLayeredServer {
            service: layered,
            router,
            _api: PhantomData,
            _provided: PhantomData,
        }
    }

    /// Finalize the server and convert to a regular [`Server`](crate::server::Server).
    ///
    /// Only compiles if all required effects have been provided.
    pub fn ready<Idx>(self) -> crate::server::Server<A>
    where
        A: AllProvided<P, Idx>,
    {
        crate::server::Server::from_router(self.router)
    }

    /// Start serving HTTP requests on the given address.
    ///
    /// Only compiles if all required effects have been provided via
    /// `.provide::<E>()` calls.
    pub async fn serve<Idx>(
        self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        A: AllProvided<P, Idx>,
    {
        self.ready::<Idx>().serve(addr).await
    }

    /// Start serving with graceful shutdown.
    ///
    /// Only compiles if all required effects have been provided.
    pub async fn serve_with_shutdown<Idx>(
        self,
        listener: tokio::net::TcpListener,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        A: AllProvided<P, Idx>,
    {
        self.ready::<Idx>()
            .serve_with_shutdown(listener, shutdown)
            .await
    }
}

/// An effectful server with Tower middleware layers applied.
///
/// Created by [`EffectfulServer::layer`]. Supports further `.provide()`,
/// `.layer()`, and `.serve()` calls.
pub struct EffectfulLayeredServer<A: ApiSpec, Provided, S> {
    service: S,
    router: Arc<Router>,
    _api: PhantomData<A>,
    _provided: PhantomData<Provided>,
}

impl<A: ApiSpec, P, S> EffectfulLayeredServer<A, P, S> {
    /// Declare that a middleware effect has been provided.
    pub fn provide<E: Effect>(self) -> EffectfulLayeredServer<A, ECons<E, P>, S> {
        EffectfulLayeredServer {
            service: self.service,
            router: self.router,
            _api: PhantomData,
            _provided: PhantomData,
        }
    }

    /// Add shared state accessible via [`State<T>`](crate::extract::State) extractors.
    pub fn with_state<T: Clone + Send + Sync + 'static>(self, state: T) -> Self {
        self.router.set_state_injector(Arc::new(move |ext| {
            ext.insert(state.clone());
        }));
        self
    }

    /// Set the maximum request body size.
    pub fn max_body_size(self, max: usize) -> Self {
        self.router.set_max_body_size(max);
        self
    }

    /// Set a path prefix for all routes.
    pub fn nest(self, prefix: &str) -> Self {
        self.router.set_prefix(prefix);
        self
    }
}

impl<A: ApiSpec, P, S> EffectfulLayeredServer<A, P, S>
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
    /// Apply another Tower middleware layer.
    pub fn layer<L>(self, layer: L) -> EffectfulLayeredServer<A, P, L::Service>
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
        EffectfulLayeredServer {
            service: layer.layer(self.service),
            router: self.router,
            _api: PhantomData,
            _provided: PhantomData,
        }
    }
}

impl<A: ApiSpec, P, S> EffectfulLayeredServer<A, P, S>
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
    /// Finalize into a [`LayeredServer`].
    ///
    /// Only compiles if all required effects have been provided.
    pub fn ready<Idx>(self) -> LayeredServer<S>
    where
        A: AllProvided<P, Idx>,
    {
        LayeredServer {
            service: self.service,
            router: self.router,
        }
    }

    /// Start serving HTTP requests.
    ///
    /// Only compiles if all required effects have been provided.
    pub async fn serve<Idx>(
        self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        A: AllProvided<P, Idx>,
    {
        self.ready::<Idx>().serve(addr).await
    }

    /// Start serving with graceful shutdown.
    ///
    /// Only compiles if all required effects have been provided.
    pub async fn serve_with_shutdown<Idx>(
        self,
        listener: tokio::net::TcpListener,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        A: AllProvided<P, Idx>,
    {
        self.ready::<Idx>()
            .serve_with_shutdown(listener, shutdown)
            .await
    }
}
