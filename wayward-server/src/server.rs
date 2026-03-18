//! The type-safe [`Server`] builder and [`serve`] convenience function.

use std::convert::Infallible;
use std::future::Future;
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

    /// Set a path prefix for all routes in this server.
    ///
    /// Only requests whose path starts with the prefix will match. The prefix
    /// is stripped before route matching.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Routes are /api/v1/hello, /api/v1/users, etc.
    /// Server::<API>::new(handlers)
    ///     .nest("/api/v1")
    ///     .serve(addr)
    ///     .await?;
    /// ```
    pub fn nest(self, prefix: &str) -> Self {
        self.router.set_prefix(prefix);
        self
    }

    /// Set the maximum request body size in bytes.
    ///
    /// Bodies exceeding this limit are rejected with 413 Payload Too Large.
    /// Default: 2 MiB (2,097,152 bytes).
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
    pub fn with_openapi(self, title: &str, version: &str) -> Self
    where
        A: wayward_openapi::ApiToSpec,
    {
        let spec = A::to_spec(title, version);
        let spec_json = std::sync::Arc::new(
            serde_json::to_string_pretty(&spec).expect("OpenAPI spec serialization failed"),
        );

        let router = &self.router;

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

    /// Serve static files from a directory at a given URL prefix.
    ///
    /// Requests to `{prefix}/{path}` will serve files from `{dir}/{path}`.
    /// 404 is returned if the file doesn't exist.
    ///
    /// # Example
    ///
    /// ```ignore
    /// Server::<API>::new(handlers)
    ///     .with_static_files("/static", "./public")
    ///     .serve(addr)
    ///     .await?;
    /// ```
    pub fn with_static_files(self, prefix: &str, dir: impl Into<std::path::PathBuf>) -> Self {
        let dir: std::path::PathBuf = dir.into();
        let prefix_segments: Vec<String> = prefix
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let prefix_len = prefix_segments.len();

        let router = &self.router;

        let dir = Arc::new(dir);
        let prefix_segs = Arc::new(prefix_segments);

        // Add a catch-all route for the prefix (matches /static, /static/, /static/foo).
        router.add_route(
            http::Method::GET,
            format!("{prefix}/{{*path}}"),
            {
                let prefix_segs = prefix_segs.clone();
                Box::new(move |segments: &[&str]| {
                    // Match: prefix exactly, or prefix + any file path
                    segments.len() >= prefix_segs.len()
                        && segments[..prefix_segs.len()]
                            .iter()
                            .zip(prefix_segs.iter())
                            .all(|(a, b)| *a == b.as_str())
                })
            },
            {
                let dir = dir.clone();
                Box::new(move |parts: http::request::Parts, _body: bytes::Bytes| {
                    let dir = dir.clone();
                    Box::pin(async move {
                        let path = parts.uri.path();
                        // Strip prefix to get the file path.
                        let file_path: String = path
                            .splitn(prefix_len + 2, '/')
                            .skip(prefix_len + 1)
                            .collect::<Vec<_>>()
                            .join("/");

                        // Prevent directory traversal.
                        if file_path.contains("..") {
                            let mut res = http::Response::new(crate::body::body_from_string(
                                "Forbidden".to_string(),
                            ));
                            *res.status_mut() = http::StatusCode::FORBIDDEN;
                            return res;
                        }

                        let full_path = if file_path.is_empty() {
                            // /static or /static/ → try index.html
                            dir.join("index.html")
                        } else {
                            let p = dir.join(&file_path);
                            // If it's a directory, try index.html inside it
                            if p.is_dir() {
                                p.join("index.html")
                            } else {
                                p
                            }
                        };

                        match tokio::fs::read(&full_path).await {
                            Ok(contents) => {
                                let mime = mime_from_path(&full_path);
                                let body =
                                    crate::body::body_from_bytes(bytes::Bytes::from(contents));
                                let mut res = http::Response::new(body);
                                if let Ok(val) = http::HeaderValue::from_str(mime) {
                                    res.headers_mut().insert(http::header::CONTENT_TYPE, val);
                                }
                                res
                            }
                            Err(_) => {
                                let mut res = http::Response::new(crate::body::body_from_string(
                                    "Not Found".to_string(),
                                ));
                                *res.status_mut() = http::StatusCode::NOT_FOUND;
                                res
                            }
                        }
                    })
                })
            },
        );

        self
    }

    /// Serve a file as the fallback for unmatched routes (SPA mode).
    ///
    /// When no API route matches, the given file (typically `index.html`)
    /// is served. This enables client-side routing in single-page apps.
    ///
    /// # Example
    ///
    /// ```ignore
    /// Server::<API>::new(handlers)
    ///     .with_static_files("/static", "./public")
    ///     .with_spa_fallback("./public/index.html")
    ///     .serve(addr)
    ///     .await?;
    /// ```
    pub fn with_spa_fallback(self, index_path: impl Into<std::path::PathBuf>) -> Self {
        let index_path: std::path::PathBuf = index_path.into();

        // Read the file once at startup and cache it.
        let html = std::fs::read_to_string(&index_path).unwrap_or_else(|e| {
            panic!(
                "failed to read SPA fallback file {}: {e}",
                index_path.display()
            )
        });
        let html = Arc::new(html);

        self.set_fallback_raw(Arc::new(move |req| {
            let html = html.clone();
            let path = req.uri().path().to_string();
            Box::pin(async move {
                // Don't serve SPA HTML for paths that look like file requests
                // (contain a dot in the last segment, e.g. /foo/bar.js).
                let last_segment = path.rsplit('/').next().unwrap_or("");
                if last_segment.contains('.') {
                    let mut res =
                        http::Response::new(crate::body::body_from_string("Not Found".to_string()));
                    *res.status_mut() = http::StatusCode::NOT_FOUND;
                    return res;
                }

                let body = crate::body::body_from_string(html.to_string());
                let mut res = http::Response::new(body);
                res.headers_mut().insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("text/html; charset=utf-8"),
                );
                res
            })
        }));

        self
    }

    /// Set a raw fallback function on the router.
    ///
    /// Used by `with_fallback` and `with_axum_fallback`.
    pub(crate) fn set_fallback_raw(&self, fallback: crate::router::FallbackService) {
        let router = &self.router;
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
    pub fn with_fallback<S>(self, service: S) -> Self
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
        let router = self.router.clone();
        let svc = RouterService::new(self.router);
        let layered = layer.layer(svc);
        LayeredServer {
            service: layered,
            router,
        }
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
        self.serve_with_shutdown(listener, std::future::pending())
            .await
    }

    /// Start serving with graceful shutdown.
    ///
    /// The server stops accepting new connections when the `shutdown` future
    /// completes. Existing connections are allowed to finish.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let server = Server::<API>::new(handlers);
    /// let listener = TcpListener::bind("0.0.0.0:3000").await?;
    ///
    /// server.serve_with_shutdown(listener, async {
    ///     tokio::signal::ctrl_c().await.ok();
    ///     eprintln!("shutting down...");
    /// }).await?;
    /// ```
    pub async fn serve_with_shutdown(
        self,
        listener: TcpListener,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                result = listener.accept() => {
                    let (stream, _) = result?;
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
                () = &mut shutdown => {
                    eprintln!("Shutting down gracefully...");
                    return Ok(());
                }
            }
        }
    }
}

/// A server with Tower middleware layers applied.
///
/// Created by [`Server::layer`]. Supports further `.layer()` calls and `.serve()`.
pub struct LayeredServer<S> {
    /// The layered service. Exposed for advanced use cases (e.g., manual serving).
    pub service: S,
    /// Reference to the underlying router for post-layer configuration.
    router: Arc<Router>,
}

impl<S> LayeredServer<S> {
    /// Add shared state accessible via [`State<T>`] extractors.
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

    /// Serve static files from a directory.
    pub fn with_static_files(self, prefix: &str, dir: impl Into<std::path::PathBuf>) -> Self {
        // Delegate to the shared router's add_route via the same logic as Server.
        let dir: std::path::PathBuf = dir.into();
        let prefix_segments: Vec<String> = prefix
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let prefix_len = prefix_segments.len();
        let dir = Arc::new(dir);
        let prefix_segs = Arc::new(prefix_segments);

        self.router.add_route(
            http::Method::GET,
            format!("{prefix}/{{*path}}"),
            {
                let prefix_segs = prefix_segs.clone();
                Box::new(move |segments: &[&str]| {
                    segments.len() >= prefix_segs.len()
                        && segments[..prefix_segs.len()]
                            .iter()
                            .zip(prefix_segs.iter())
                            .all(|(a, b)| *a == b.as_str())
                })
            },
            {
                let dir = dir.clone();
                Box::new(move |parts: http::request::Parts, _body: bytes::Bytes| {
                    let dir = dir.clone();
                    Box::pin(async move {
                        let path = parts.uri.path();
                        let file_path: String = path
                            .splitn(prefix_len + 2, '/')
                            .skip(prefix_len + 1)
                            .collect::<Vec<_>>()
                            .join("/");
                        if file_path.contains("..") {
                            let mut res = http::Response::new(crate::body::body_from_string(
                                "Forbidden".to_string(),
                            ));
                            *res.status_mut() = http::StatusCode::FORBIDDEN;
                            return res;
                        }
                        let full_path = if file_path.is_empty() {
                            dir.join("index.html")
                        } else {
                            let p = dir.join(&file_path);
                            if p.is_dir() {
                                p.join("index.html")
                            } else {
                                p
                            }
                        };
                        match tokio::fs::read(&full_path).await {
                            Ok(contents) => {
                                let mime = mime_from_path(&full_path);
                                let body =
                                    crate::body::body_from_bytes(bytes::Bytes::from(contents));
                                let mut res = http::Response::new(body);
                                if let Ok(val) = http::HeaderValue::from_str(mime) {
                                    res.headers_mut().insert(http::header::CONTENT_TYPE, val);
                                }
                                res
                            }
                            Err(_) => {
                                let mut res = http::Response::new(crate::body::body_from_string(
                                    "Not Found".to_string(),
                                ));
                                *res.status_mut() = http::StatusCode::NOT_FOUND;
                                res
                            }
                        }
                    })
                })
            },
        );
        self
    }

    /// Serve a file as SPA fallback for unmatched routes.
    pub fn with_spa_fallback(self, index_path: impl Into<std::path::PathBuf>) -> Self {
        let index_path: std::path::PathBuf = index_path.into();
        let html = std::fs::read_to_string(&index_path).unwrap_or_else(|e| {
            panic!(
                "failed to read SPA fallback file {}: {e}",
                index_path.display()
            )
        });
        let html = Arc::new(html);
        self.router.set_fallback(Arc::new(move |req| {
            let html = html.clone();
            let path = req.uri().path().to_string();
            Box::pin(async move {
                let last_segment = path.rsplit('/').next().unwrap_or("");
                if last_segment.contains('.') {
                    let mut res =
                        http::Response::new(crate::body::body_from_string("Not Found".to_string()));
                    *res.status_mut() = http::StatusCode::NOT_FOUND;
                    return res;
                }
                let body = crate::body::body_from_string(html.to_string());
                let mut res = http::Response::new(body);
                res.headers_mut().insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("text/html; charset=utf-8"),
                );
                res
            })
        }));
        self
    }
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
            router: self.router,
        }
    }

    /// Start serving HTTP requests on the given address.
    pub async fn serve(
        self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(addr).await?;
        eprintln!("Listening on http://{addr}");
        self.serve_with_shutdown(listener, std::future::pending())
            .await
    }

    /// Start serving with graceful shutdown.
    pub async fn serve_with_shutdown(
        self,
        listener: TcpListener,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                result = listener.accept() => {
                    let (stream, _) = result?;
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
                () = &mut shutdown => {
                    eprintln!("Shutting down gracefully...");
                    return Ok(());
                }
            }
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

/// Guess MIME type from file extension.
fn mime_from_path(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("txt") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}
