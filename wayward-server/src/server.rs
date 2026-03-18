//! The type-safe [`Server`] builder and [`serve`] convenience function.

use std::convert::Infallible;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;

use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use wayward_core::ApiSpec;

use crate::body::BoxBody;
use crate::router::{Router, RouterService};
use crate::serves::Serves;

/// A type-safe HTTP server parameterized by an API specification.
///
/// The `A` type parameter is the API type — a tuple of endpoints. The server
/// ensures at compile time (via [`Serves`]) that every endpoint has a handler.
///
/// # Example
///
/// ```ignore
/// type API = (
///     GetEndpoint<path!("hello"), String>,
/// );
///
/// let server = Server::<API>::new((hello_handler,));
/// server.serve("127.0.0.1:3000".parse().unwrap()).await?;
/// ```
pub struct Server<A: ApiSpec> {
    router: Arc<Router>,
    _api: PhantomData<A>,
}

impl<A: ApiSpec> Server<A> {
    /// Create a new server with handlers covering the full API.
    ///
    /// Fails to compile if the handler tuple doesn't match the API type.
    pub fn new<H: Serves<A>>(handlers: H) -> Self {
        let mut router = Router::new();
        handlers.register(&mut router);
        Server {
            router: Arc::new(router),
            _api: PhantomData,
        }
    }

    /// Add shared state accessible via [`State<T>`](crate::extract::State) extractors.
    pub fn with_state<T: Clone + Send + Sync + 'static>(mut self, state: T) -> Self {
        let router = Arc::get_mut(&mut self.router)
            .expect("with_state must be called before cloning the router");
        router.set_state_injector(Arc::new(move |ext| {
            ext.insert(state.clone());
        }));
        self
    }

    /// Enable OpenAPI spec serving at `/openapi.json` and Swagger UI at `/docs`.
    ///
    /// Requires `feature = "openapi"` and that the API type implements
    /// [`ApiToSpec`](wayward_openapi::ApiToSpec).
    ///
    /// # Example
    ///
    /// ```ignore
    /// Server::<API>::new(handlers)
    ///     .with_openapi("My API", "1.0.0")
    ///     .serve(addr)
    ///     .await?;
    /// ```
    #[cfg(feature = "openapi")]
    pub fn with_openapi(mut self, title: &str, version: &str) -> Self
    where
        A: wayward_openapi::ApiToSpec,
    {
        let spec = A::to_spec(title, version);
        let spec_json = std::sync::Arc::new(
            serde_json::to_string_pretty(&spec).expect("OpenAPI spec serialization failed"),
        );

        let router = std::sync::Arc::get_mut(&mut self.router)
            .expect("with_openapi must be called before cloning the router");

        let spec_json_str =
            serde_json::to_string(&spec).expect("OpenAPI spec serialization failed");

        router.add_route(
            http::Method::GET,
            "/openapi.json".to_string(),
            crate::openapi::exact_match(&["openapi.json"]),
            crate::openapi::spec_handler(spec_json.clone()),
        );

        router.add_route(
            http::Method::GET,
            "/docs".to_string(),
            crate::openapi::exact_match(&["docs"]),
            crate::openapi::docs_handler(title, version, &spec_json_str),
        );

        self
    }

    /// Set a raw fallback function on the router.
    ///
    /// Used by `with_fallback` and `with_axum_fallback`.
    pub(crate) fn set_fallback_raw(&mut self, fallback: crate::router::FallbackService) {
        let router = Arc::get_mut(&mut self.router)
            .expect("set_fallback must be called before cloning the router");
        router.set_fallback(fallback);
    }

    /// Set a fallback Tower service for requests that don't match any wayward route.
    ///
    /// This enables embedding an Axum router (or any Tower service) inside
    /// a wayward server — the reverse of `into_axum_router()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let axum_routes = axum::Router::new()
    ///     .route("/health", get(|| async { "ok" }));
    ///
    /// Server::<API>::new(handlers)
    ///     .with_fallback(axum_routes)
    ///     .serve(addr)
    ///     .await?;
    /// ```
    pub fn with_fallback<S>(mut self, service: S) -> Self
    where
        S: tower_service::Service<
                http::Request<hyper::body::Incoming>,
                Response = http::Response<BoxBody>,
                Error = Infallible,
            > + Clone
            + Send
            + Sync
            + 'static,
        S::Future: Send + 'static,
    {
        self.set_fallback_raw(Arc::new(
            move |req: http::Request<hyper::body::Incoming>| {
                let mut svc = service.clone();
                Box::pin(async move {
                    tower_service::Service::call(&mut svc, req)
                        .await
                        .unwrap_or_else(|e| match e {})
                })
            },
        ));
        self
    }

    /// Apply a Tower middleware layer to the server.
    ///
    /// The layer wraps the entire router service. This is the same API
    /// as Axum's `.layer()` — any `tower::Layer` that accepts the router
    /// service type can be used.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use tower_http::trace::TraceLayer;
    /// use tower_http::cors::CorsLayer;
    ///
    /// Server::<API>::new(handlers)
    ///     .layer(TraceLayer::new_for_http())
    ///     .layer(CorsLayer::permissive())
    ///     .serve(addr)
    ///     .await?;
    /// ```
    pub fn layer<L>(self, layer: L) -> LayeredServer<L::Service>
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
        let svc = RouterService::new(self.router);
        let layered = layer.layer(svc);
        LayeredServer { service: layered }
    }

    /// Get the inner [`RouterService`] as a Tower service.
    pub fn into_service(self) -> RouterService {
        RouterService::new(self.router)
    }

    /// Get the inner router (for integration with other frameworks).
    pub fn into_router(self) -> Router {
        Arc::try_unwrap(self.router).unwrap_or_else(|_| {
            panic!("cannot unwrap router — it has been cloned");
        })
    }

    /// Start serving HTTP requests on the given address.
    pub async fn serve(
        self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(addr).await?;
        eprintln!("Listening on http://{addr}");

        loop {
            let (stream, _) = listener.accept().await?;
            let io = TokioIo::new(stream);
            let svc = RouterService::new(self.router.clone());
            let hyper_svc = hyper_util::service::TowerToHyperService::new(svc);

            tokio::task::spawn(async move {
                if let Err(e) = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, hyper_svc)
                    .await
                {
                    eprintln!("Connection error: {e}");
                }
            });
        }
    }
}

/// A server with Tower middleware layers applied.
///
/// Created by [`Server::layer`]. Supports further `.layer()` calls and `.serve()`.
pub struct LayeredServer<S> {
    /// The layered service. Exposed for advanced use cases (e.g., manual serving).
    pub service: S,
}

impl<S> LayeredServer<S>
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
    pub fn layer<L>(self, layer: L) -> LayeredServer<L::Service>
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
        LayeredServer {
            service: layer.layer(self.service),
        }
    }

    /// Start serving HTTP requests on the given address.
    pub async fn serve(
        self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(addr).await?;
        eprintln!("Listening on http://{addr}");

        loop {
            let (stream, _) = listener.accept().await?;
            let io = TokioIo::new(stream);
            let svc = self.service.clone();
            let hyper_svc = hyper_util::service::TowerToHyperService::new(svc);

            tokio::task::spawn(async move {
                if let Err(e) = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, hyper_svc)
                    .await
                {
                    eprintln!("Connection error: {e}");
                }
            });
        }
    }
}

/// Convenience function to create and serve an API.
///
/// # Example
///
/// ```ignore
/// serve::<API, _>("127.0.0.1:3000".parse().unwrap(), (handler1, handler2)).await?;
/// ```
pub async fn serve<A: ApiSpec, H: Serves<A>>(
    addr: SocketAddr,
    handlers: H,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Server::<A>::new(handlers).serve(addr).await
}
