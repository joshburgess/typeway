//! The runtime [`Router`] that matches incoming requests to handlers.
//!
//! The router performs a linear scan over registered routes, matching by
//! HTTP method and path pattern. For typical API sizes (<100 routes),
//! this is faster than a trie or hash map.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use http::StatusCode;

use crate::body::{body_from_string, BoxBody};
use crate::extract::PathSegments;
use crate::handler::BoxedHandler;

pub(crate) type MatchFn = Box<dyn Fn(&[&str]) -> bool + Send + Sync>;
type StateInjector = Arc<dyn Fn(&mut http::Extensions) + Send + Sync>;
pub(crate) type FallbackService = Arc<
    dyn Fn(
            http::Request<hyper::body::Incoming>,
        ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>>
        + Send
        + Sync,
>;

/// Default maximum request body size: 2 MiB.
pub const DEFAULT_MAX_BODY_SIZE: usize = 2 * 1024 * 1024;

/// A runtime HTTP router.
///
/// Routes are stored in a flat list but indexed by HTTP method for fast
/// dispatch. For typical API sizes (<100 routes), this linear scan with
/// method pre-filtering is faster than a trie.
pub struct Router {
    routes: Vec<RouteEntry>,
    /// Routes indexed by method for O(1) method filtering.
    method_index: MethodIndex,
    state_injector: Option<StateInjector>,
    fallback: Option<FallbackService>,
    /// Maximum request body size in bytes. Bodies exceeding this are rejected
    /// with 413 Payload Too Large.
    max_body_size: usize,
    /// Optional path prefix. When set, only requests whose path starts with
    /// this prefix are matched, and the prefix is stripped before route matching.
    prefix: Option<Vec<String>>,
}

struct RouteEntry {
    #[allow(dead_code)]
    pattern: String,
    /// Optional first literal segment for fast prefix rejection.
    first_segment: Option<String>,
    match_fn: MatchFn,
    handler: BoxedHandler,
}

/// Pre-computed index: for each HTTP method, the indices into `routes`.
#[derive(Default)]
struct MethodIndex {
    get: Vec<usize>,
    post: Vec<usize>,
    put: Vec<usize>,
    delete: Vec<usize>,
    patch: Vec<usize>,
    head: Vec<usize>,
    options: Vec<usize>,
    other: Vec<usize>,
}

impl MethodIndex {
    fn get_indices(&self, method: &http::Method) -> &[usize] {
        match *method {
            http::Method::GET => &self.get,
            http::Method::POST => &self.post,
            http::Method::PUT => &self.put,
            http::Method::DELETE => &self.delete,
            http::Method::PATCH => &self.patch,
            http::Method::HEAD => &self.head,
            http::Method::OPTIONS => &self.options,
            _ => &self.other,
        }
    }

    fn get_all_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.get
            .iter()
            .chain(&self.post)
            .chain(&self.put)
            .chain(&self.delete)
            .chain(&self.patch)
            .chain(&self.head)
            .chain(&self.options)
            .chain(&self.other)
            .copied()
    }

    fn push(&mut self, method: &http::Method, idx: usize) {
        match *method {
            http::Method::GET => self.get.push(idx),
            http::Method::POST => self.post.push(idx),
            http::Method::PUT => self.put.push(idx),
            http::Method::DELETE => self.delete.push(idx),
            http::Method::PATCH => self.patch.push(idx),
            http::Method::HEAD => self.head.push(idx),
            http::Method::OPTIONS => self.options.push(idx),
            _ => self.other.push(idx),
        }
    }
}

impl Router {
    /// Create an empty router.
    pub fn new() -> Self {
        Router {
            routes: Vec::new(),
            method_index: MethodIndex::default(),
            state_injector: None,
            fallback: None,
            max_body_size: DEFAULT_MAX_BODY_SIZE,
            prefix: None,
        }
    }

    /// Set a path prefix for all routes.
    ///
    /// Only requests starting with this prefix will be matched, and the
    /// prefix segments are stripped before route matching.
    pub(crate) fn set_prefix(&mut self, prefix: &str) {
        let segments: Vec<String> = prefix
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        if !segments.is_empty() {
            self.prefix = Some(segments);
        }
    }

    /// Set the maximum request body size in bytes.
    ///
    /// Bodies exceeding this limit are rejected with 413 Payload Too Large
    /// before the handler is called. Default: 2 MiB.
    pub(crate) fn set_max_body_size(&mut self, max: usize) {
        self.max_body_size = max;
    }

    /// Register a route with a method, pattern, match function, and handler.
    pub(crate) fn add_route(
        &mut self,
        method: http::Method,
        pattern: String,
        match_fn: MatchFn,
        handler: BoxedHandler,
    ) {
        // Extract first literal segment from pattern for fast prefix filtering.
        let first_segment = pattern
            .split('/')
            .find(|s| !s.is_empty() && !s.starts_with('{'))
            .map(|s| s.to_string());

        let idx = self.routes.len();
        self.method_index.push(&method, idx);
        self.routes.push(RouteEntry {
            pattern,
            first_segment,
            match_fn,
            handler,
        });
    }

    /// Set the state injector function.
    pub(crate) fn set_state_injector(
        &mut self,
        injector: Arc<dyn Fn(&mut http::Extensions) + Send + Sync>,
    ) {
        self.state_injector = Some(injector);
    }

    /// Set a fallback service for requests that don't match any wayward route.
    pub(crate) fn set_fallback(&mut self, fallback: FallbackService) {
        self.fallback = Some(fallback);
    }

    /// Route a request to the appropriate handler.
    ///
    /// Must be called on `Arc<Router>` so the router outlives the returned future.
    /// The body is collected into bytes before handler dispatch, enabling
    /// both Hyper and Axum body types to be handled uniformly.
    pub fn route(
        self: &Arc<Self>,
        req: http::Request<hyper::body::Incoming>,
    ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
        let path = req.uri().path().to_string();
        let all_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        // Strip prefix if configured.
        let segments: &[&str] = if let Some(ref prefix) = self.prefix {
            if all_segments.len() >= prefix.len()
                && all_segments[..prefix.len()]
                    .iter()
                    .zip(prefix.iter())
                    .all(|(a, b)| *a == b.as_str())
            {
                &all_segments[prefix.len()..]
            } else {
                // Prefix doesn't match — fall through to 404/fallback.
                return if let Some(ref fallback) = self.fallback {
                    fallback(req)
                } else {
                    Box::pin(async move {
                        let mut res =
                            http::Response::new(body_from_string("Not Found".to_string()));
                        *res.status_mut() = StatusCode::NOT_FOUND;
                        res
                    })
                };
            }
        } else {
            &all_segments
        };
        let first_seg = segments.first().copied();

        let method = req.method();

        // Fast path: check only routes with matching method.
        let method_indices = self.method_index.get_indices(method);
        for &i in method_indices {
            let entry = &self.routes[i];
            // Fast prefix rejection: if the route starts with a literal segment
            // and it doesn't match the request's first segment, skip.
            if let Some(ref first) = entry.first_segment {
                if first_seg != Some(first.as_str()) {
                    continue;
                }
            }
            if (entry.match_fn)(segments) {
                let (mut parts, body) = req.into_parts();

                parts.extensions.insert(PathSegments(Arc::new(
                    segments.iter().map(|s| s.to_string()).collect(),
                )));

                if let Some(ref injector) = self.state_injector {
                    injector(&mut parts.extensions);
                }

                let router = self.clone();
                let max_body = self.max_body_size;
                return Box::pin(async move {
                    let body_bytes = match collect_body_limited(body, max_body).await {
                        Ok(bytes) => bytes,
                        Err(resp) => return resp,
                    };
                    (router.routes[i].handler)(parts, body_bytes).await
                });
            }
        }

        // No method match — check if any route matches the path (for 405 vs 404).
        let path_matched = self
            .method_index
            .get_all_indices()
            .any(|i| (self.routes[i].match_fn)(segments));

        if path_matched {
            Box::pin(async move {
                let mut res =
                    http::Response::new(body_from_string("Method Not Allowed".to_string()));
                *res.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
                res
            })
        } else if let Some(ref fallback) = self.fallback {
            fallback(req)
        } else {
            Box::pin(async move {
                let mut res = http::Response::new(body_from_string("Not Found".to_string()));
                *res.status_mut() = StatusCode::NOT_FOUND;
                res
            })
        }
    }

    /// Route a request with pre-collected body bytes.
    ///
    /// Used by the Axum interop adapter where the body has already been
    /// collected from Axum's body type.
    #[cfg(feature = "axum-interop")]
    pub(crate) async fn route_with_bytes(
        self: &Arc<Self>,
        mut parts: http::request::Parts,
        body_bytes: bytes::Bytes,
    ) -> http::Response<BoxBody> {
        let path = parts.uri.path().to_string();
        let all_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        // Strip prefix if configured.
        let segments: &[&str] = if let Some(ref prefix) = self.prefix {
            if all_segments.len() >= prefix.len()
                && all_segments[..prefix.len()]
                    .iter()
                    .zip(prefix.iter())
                    .all(|(a, b)| *a == b.as_str())
            {
                &all_segments[prefix.len()..]
            } else {
                let mut res = http::Response::new(body_from_string("Not Found".to_string()));
                *res.status_mut() = StatusCode::NOT_FOUND;
                return res;
            }
        } else {
            &all_segments
        };
        let first_seg = segments.first().copied();

        let method = &parts.method;
        let method_indices = self.method_index.get_indices(method);

        for &i in method_indices {
            let entry = &self.routes[i];
            if let Some(ref first) = entry.first_segment {
                if first_seg != Some(first.as_str()) {
                    continue;
                }
            }
            if (entry.match_fn)(segments) {
                parts.extensions.insert(PathSegments(Arc::new(
                    segments.iter().map(|s| s.to_string()).collect(),
                )));

                if let Some(ref injector) = self.state_injector {
                    injector(&mut parts.extensions);
                }

                return (entry.handler)(parts, body_bytes).await;
            }
        }

        let path_matched = self
            .method_index
            .get_all_indices()
            .any(|i| (self.routes[i].match_fn)(segments));

        if path_matched {
            let mut res = http::Response::new(body_from_string("Method Not Allowed".to_string()));
            *res.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
            res
        } else {
            let mut res = http::Response::new(body_from_string("Not Found".to_string()));
            *res.status_mut() = StatusCode::NOT_FOUND;
            res
        }
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Body collection with size limit
// ---------------------------------------------------------------------------

/// Collect a hyper body into bytes, enforcing a size limit.
///
/// Returns 413 Payload Too Large if the body exceeds `max_bytes`.
async fn collect_body_limited(
    body: hyper::body::Incoming,
    max_bytes: usize,
) -> Result<bytes::Bytes, http::Response<BoxBody>> {
    use http_body_util::BodyExt;

    let limited = http_body_util::Limited::new(body, max_bytes);
    match limited.collect().await {
        Ok(collected) => Ok(collected.to_bytes()),
        Err(_) => {
            let mut res = http::Response::new(body_from_string(format!(
                "request body too large (max {max_bytes} bytes)"
            )));
            *res.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
            Err(res)
        }
    }
}

// ---------------------------------------------------------------------------
// Tower Service implementation
// ---------------------------------------------------------------------------

/// A [`tower::Service`] wrapper around a shared [`Router`].
///
/// This enables applying Tower middleware layers (tracing, CORS, compression,
/// timeouts, etc.) to the wayward router.
#[derive(Clone)]
pub struct RouterService {
    router: Arc<Router>,
}

impl RouterService {
    /// Wrap a router in a Tower service.
    pub fn new(router: Arc<Router>) -> Self {
        RouterService { router }
    }
}

impl tower_service::Service<http::Request<hyper::body::Incoming>> for RouterService {
    type Response = http::Response<BoxBody>;
    type Error = std::convert::Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<hyper::body::Incoming>) -> Self::Future {
        let router = self.router.clone();
        Box::pin(async move { Ok(router.route(req).await) })
    }
}
