//! Axum interoperability — embed a typeway API inside an Axum app.
//!
//! Enabled with `feature = "axum-interop"`. Provides conversions from
//! typeway's [`Server`] and [`Router`] into Axum types.
//!
//! # Example
//!
//! ```ignore
//! use axum::routing::get;
//!
//! let typeway_api = Server::<MyAPI>::new(handlers);
//! let app = axum::Router::new()
//!     .nest("/api/v1", typeway_api.into_axum_router())
//!     .route("/health", get(|| async { "ok" }));
//! ```

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use http_body_util::BodyExt;

use typeway_core::ApiSpec;

use crate::body::{body_from_bytes, BoxBody};
use crate::router::Router;
use crate::server::Server;

impl<A: ApiSpec> Server<A> {
    /// Convert this server into an Axum [`Router`](axum::Router).
    ///
    /// The returned router handles all routes defined in the API type.
    /// It can be nested into a larger Axum application at any path prefix.
    pub fn into_axum_router(self) -> axum::Router {
        let adapter = AxumAdapter::new(Arc::new(self.into_router()));
        axum::Router::new().fallback_service(adapter)
    }
}

impl Router {
    /// Convert this router into an Axum [`Router`](axum::Router).
    pub fn into_axum_router(self) -> axum::Router {
        let adapter = AxumAdapter::new(Arc::new(self));
        axum::Router::new().fallback_service(adapter)
    }
}

impl<A: ApiSpec> From<Server<A>> for axum::Router {
    fn from(server: Server<A>) -> axum::Router {
        server.into_axum_router()
    }
}

impl<A: ApiSpec> Server<A> {
    /// Embed an Axum [`Router`](axum::Router) as a fallback for unmatched routes.
    ///
    /// Typeway routes are checked first. If no typeway route matches, the
    /// request is forwarded to the Axum router. This is the reverse of
    /// [`into_axum_router`](Server::into_axum_router).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let axum_routes = axum::Router::new()
    ///     .route("/health", get(|| async { "ok" }));
    ///
    /// Server::<API>::new(handlers)
    ///     .with_axum_fallback(axum_routes)
    ///     .serve(addr)
    ///     .await?;
    /// ```
    pub fn with_axum_fallback(self, axum_router: axum::Router) -> Self {
        self.set_fallback_raw(Arc::new(
            move |req: http::Request<hyper::body::Incoming>| {
                let mut axum_svc = axum_router.clone();
                Box::pin(async move {
                    // Convert Request<Incoming> -> Request<axum::body::Body>
                    let (parts, incoming) = req.into_parts();
                    let axum_body = axum::body::Body::new(incoming);
                    let axum_req = http::Request::from_parts(parts, axum_body);

                    // Call the Axum router.
                    let axum_resp = tower_service::Service::call(&mut axum_svc, axum_req)
                        .await
                        .unwrap_or_else(|e| match e {});

                    // Convert Response<axum::body::Body> -> Response<BoxBody>
                    let (parts, body) = axum_resp.into_parts();
                    let body_bytes = body
                        .collect()
                        .await
                        .map(|c| c.to_bytes())
                        .unwrap_or_default();
                    http::Response::from_parts(parts, body_from_bytes(body_bytes))
                })
            },
        ));

        self
    }
}

/// Adapter that bridges Axum's body type to typeway's router.
///
/// Axum uses `axum::body::Body` while our router expects `hyper::body::Incoming`.
/// This adapter collects the incoming body bytes and dispatches to the
/// typeway router's handlers directly.
#[derive(Clone)]
struct AxumAdapter {
    router: Arc<Router>,
}

impl AxumAdapter {
    fn new(router: Arc<Router>) -> Self {
        Self { router }
    }
}

impl tower_service::Service<http::Request<axum::body::Body>> for AxumAdapter {
    type Response = http::Response<BoxBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<axum::body::Body>) -> Self::Future {
        let router = self.router.clone();

        Box::pin(async move {
            let (parts, body) = req.into_parts();

            // Collect the axum body bytes.
            let body_bytes = body
                .collect()
                .await
                .map(|c| c.to_bytes())
                .unwrap_or_default();

            // Dispatch through the router's internal handler matching.
            Ok(router.route_with_bytes(parts, body_bytes).await)
        })
    }
}
