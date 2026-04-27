//! The runtime [`Router`] that matches incoming requests to handlers.
//!
//! Routes are dispatched via a per-method radix trie (`matchit`). Patterns
//! that conflict with already-registered routes fall back to a linear scan
//! within their method bucket, so registration never fails silently. The
//! `match_fn` produced by `typeway_path!` runs on the candidate route as a
//! type-validation step (e.g. confirming `{}` parses as `u32`).

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use http::StatusCode;

use crate::body::{body_from_bytes, body_from_string, BoxBody};
use crate::extract::PathPrefixOffset;
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
/// Routes are dispatched through a per-method radix trie (`matchit`). The
/// trie matches on the request path; a per-route `match_fn` then validates
/// typed captures (e.g. that `{}` parses as `u32`). Patterns that conflict
/// in the trie fall back to a linear scan within the same method bucket,
/// so registration never silently drops a route.
pub struct Router {
    /// All mutable state behind RwLock so the router can be configured
    /// after Arc is shared (e.g., when LayeredServer wraps it).
    inner: parking_lot::RwLock<RouterInner>,
}

struct RouterInner {
    routes: Vec<RouteEntry>,
    method_index: MethodIndex,
    /// Unified path-shape trie across all methods. Used to short-circuit
    /// 404 detection without walking each method bucket.
    any_method_trie: matchit::Router<()>,
    /// Patterns already mirrored into `any_method_trie`; needed because
    /// matchit rejects duplicate inserts (same pattern from another method)
    /// the same way it rejects real structural conflicts.
    any_method_seen: HashSet<String>,
    /// Set when a pattern conflicted with `any_method_trie` (different shape
    /// collapsing onto an existing entry). When true, miss detection falls
    /// back to the per-bucket walk so we don't false-negative.
    any_method_has_fallback: bool,
    /// Bitmap of which method buckets have routes registered. When the
    /// request's method bit is the *only* one set, a miss in that bucket
    /// is a definite 404 with no need to consult `any_method_trie` (saves
    /// a trie walk on the miss path for single-method APIs).
    methods_present: u8,
    state_injector: Option<StateInjector>,
    fallback: Option<FallbackService>,
    max_body_size: usize,
    prefix: Option<Vec<String>>,
    /// Cached `"/seg1/seg2"` form of `prefix`, for byte-level path stripping.
    prefix_str: Option<String>,
}

struct RouteEntry {
    // method/pattern are only read by `find_handler_by_pattern` (gRPC feature).
    #[cfg_attr(not(feature = "grpc"), allow(dead_code))]
    method: http::Method,
    #[cfg_attr(not(feature = "grpc"), allow(dead_code))]
    pattern: String,
    match_fn: MatchFn,
    handler: BoxedHandler,
}

/// Per-method radix tries plus a linear fallback for patterns matchit rejects.
#[derive(Default)]
struct MethodIndex {
    get: MethodBucket,
    post: MethodBucket,
    put: MethodBucket,
    delete: MethodBucket,
    patch: MethodBucket,
    head: MethodBucket,
    options: MethodBucket,
    other: MethodBucket,
}

#[derive(Default)]
struct MethodBucket {
    /// Radix trie of patterns -> route index. Most routes live here.
    trie: matchit::Router<usize>,
    /// Routes whose patterns conflicted with the trie (linear fallback).
    fallback: Vec<usize>,
}

impl MethodIndex {
    fn bucket(&self, method: &http::Method) -> &MethodBucket {
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

    fn bucket_mut(&mut self, method: &http::Method) -> &mut MethodBucket {
        match *method {
            http::Method::GET => &mut self.get,
            http::Method::POST => &mut self.post,
            http::Method::PUT => &mut self.put,
            http::Method::DELETE => &mut self.delete,
            http::Method::PATCH => &mut self.patch,
            http::Method::HEAD => &mut self.head,
            http::Method::OPTIONS => &mut self.options,
            _ => &mut self.other,
        }
    }

    fn all_buckets(&self) -> [&MethodBucket; 8] {
        [
            &self.get,
            &self.post,
            &self.put,
            &self.delete,
            &self.patch,
            &self.head,
            &self.options,
            &self.other,
        ]
    }
}

/// Bit position for `RouterInner::methods_present` for a given HTTP method.
fn method_bit(method: &http::Method) -> u8 {
    match *method {
        http::Method::GET => 1 << 0,
        http::Method::POST => 1 << 1,
        http::Method::PUT => 1 << 2,
        http::Method::DELETE => 1 << 3,
        http::Method::PATCH => 1 << 4,
        http::Method::HEAD => 1 << 5,
        http::Method::OPTIONS => 1 << 6,
        _ => 1 << 7,
    }
}

/// Convert a typeway pattern (`/users/{}/posts/{*rest}`) into the matchit
/// pattern syntax (`/users/{p0}/posts/{*rest}`). matchit requires every
/// capture to have a unique name; typeway emits empty `{}` for typed
/// captures and `{*name}` for catch-alls.
fn to_matchit_pattern(pat: &str) -> String {
    let bytes = pat.as_bytes();
    let mut out = String::with_capacity(pat.len() + 8);
    let mut counter: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let close = bytes[i + 1..].iter().position(|&b| b == b'}');
            match close {
                Some(rel_end) => {
                    let inner = &pat[i + 1..i + 1 + rel_end];
                    if inner.is_empty() {
                        out.push_str(&format!("{{p{counter}}}"));
                        counter += 1;
                    } else {
                        out.push('{');
                        out.push_str(inner);
                        out.push('}');
                    }
                    i += 2 + rel_end;
                }
                None => {
                    out.push_str(&pat[i..]);
                    break;
                }
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Strip the configured prefix off the request path, returning the
/// post-prefix path (always starts with `/`) or `None` if the path
/// doesn't fall under the prefix.
fn strip_prefix<'a>(prefix_str: Option<&str>, path: &'a str) -> Option<&'a str> {
    match prefix_str {
        Some(prefix) => match path.strip_prefix(prefix) {
            Some("") => Some("/"),
            Some(rest) if rest.starts_with('/') => Some(rest),
            _ => None,
        },
        None => Some(if path.is_empty() { "/" } else { path }),
    }
}

/// Lazy holder for the path segments needed by `match_fn`.
///
/// Splitting `/a/b/c` into a `SmallVec<[&str; 8]>` is cheap but not free, and
/// the most common 404 (path doesn't exist in any method) never needs the
/// segments at all — the trie miss alone is conclusive. Building them on first
/// `get()` saves the allocation in that case.
struct LazySegments<'a> {
    path: &'a str,
    cache: Option<smallvec::SmallVec<[&'a str; 8]>>,
}

impl<'a> LazySegments<'a> {
    fn new(path: &'a str) -> Self {
        Self { path, cache: None }
    }

    fn get(&mut self) -> &[&'a str] {
        self.cache
            .get_or_insert_with(|| self.path.split('/').filter(|s| !s.is_empty()).collect())
            .as_slice()
    }
}

/// Find the matching route index for a given path within a method bucket.
/// Tries the trie first, then falls back to a linear scan over conflicts.
///
/// Segments are produced lazily: on a clean trie miss with an empty fallback
/// bucket, we never allocate them at all (the common 404 path).
fn lookup_in_bucket(
    bucket: &MethodBucket,
    routes: &[RouteEntry],
    lookup_path: &str,
    segments: &mut LazySegments,
) -> Option<usize> {
    if let Ok(m) = bucket.trie.at(lookup_path) {
        let idx = *m.value;
        if (routes[idx].match_fn)(segments.get()) {
            return Some(idx);
        }
    }
    if bucket.fallback.is_empty() {
        return None;
    }
    let segs = segments.get();
    bucket
        .fallback
        .iter()
        .copied()
        .find(|&i| (routes[i].match_fn)(segs))
}

fn make_status_response(status: StatusCode, body: &'static [u8]) -> http::Response<BoxBody> {
    let mut res = http::Response::new(body_from_bytes(bytes::Bytes::from_static(body)));
    *res.status_mut() = status;
    res
}

fn not_found_response() -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
    Box::pin(std::future::ready(make_status_response(
        StatusCode::NOT_FOUND,
        b"Not Found",
    )))
}

fn method_not_allowed_response() -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
    Box::pin(std::future::ready(make_status_response(
        StatusCode::METHOD_NOT_ALLOWED,
        b"Method Not Allowed",
    )))
}

/// Outcome of resolving a request against the routing table.
enum LookupOutcome {
    Hit(usize),
    MethodNotAllowed,
    NotFound,
}

/// Run the full lookup (per-method trie, fallback, then 404-vs-405 disambiguation)
/// for `lookup_path` under `method`. Builds the segments slice lazily, so a clean
/// 404 with no fallback never allocates one.
fn resolve(inner: &RouterInner, method: &http::Method, lookup_path: &str) -> LookupOutcome {
    let mut segments = LazySegments::new(lookup_path);

    let bucket = inner.method_index.bucket(method);
    if let Some(i) = lookup_in_bucket(bucket, &inner.routes, lookup_path, &mut segments) {
        return LookupOutcome::Hit(i);
    }

    // Fast path: if our method bucket is the only populated one, no other
    // method can possibly accept this path, so a bucket miss is a guaranteed
    // 404. Skips the unified-trie lookup AND the per-bucket walk.
    let bit = method_bit(method);
    if inner.methods_present == bit {
        return LookupOutcome::NotFound;
    }

    // General case: if no pattern in any method matches the path *shape*, it's
    // definitely a 404. The unified `any_method_trie` lets us check this in one
    // trie lookup. If a structural conflict was recorded at registration, the
    // unified trie may have a false negative, so we skip the optimization.
    let any_path_shape =
        inner.any_method_has_fallback || inner.any_method_trie.at(lookup_path).is_ok();
    let path_matched = any_path_shape
        && inner
            .method_index
            .all_buckets()
            .iter()
            .any(|b| lookup_in_bucket(b, &inner.routes, lookup_path, &mut segments).is_some());

    if path_matched {
        LookupOutcome::MethodNotAllowed
    } else {
        LookupOutcome::NotFound
    }
}

impl Router {
    /// Create an empty router.
    pub fn new() -> Self {
        Router {
            inner: parking_lot::RwLock::new(RouterInner {
                routes: Vec::new(),
                method_index: MethodIndex::default(),
                any_method_trie: matchit::Router::new(),
                any_method_seen: HashSet::new(),
                any_method_has_fallback: false,
                methods_present: 0,
                state_injector: None,
                fallback: None,
                max_body_size: DEFAULT_MAX_BODY_SIZE,
                prefix: None,
                prefix_str: None,
            }),
        }
    }

    /// Set a path prefix for all routes.
    pub(crate) fn set_prefix(&self, prefix: &str) {
        let segments: Vec<String> = prefix
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        if segments.is_empty() {
            return;
        }
        let mut joined = String::with_capacity(segments.iter().map(|s| s.len() + 1).sum());
        for seg in &segments {
            joined.push('/');
            joined.push_str(seg);
        }
        let mut inner = self.inner.write();
        inner.prefix = Some(segments);
        inner.prefix_str = Some(joined);
    }

    /// Set the maximum request body size in bytes.
    pub(crate) fn set_max_body_size(&self, max: usize) {
        self.inner.write().max_body_size = max;
    }

    /// Register a route with a method, pattern, match function, and handler.
    pub(crate) fn add_route(
        &self,
        method: http::Method,
        pattern: String,
        match_fn: MatchFn,
        handler: BoxedHandler,
    ) {
        let mut inner = self.inner.write();
        let idx = inner.routes.len();

        inner.methods_present |= method_bit(&method);

        let matchit_pattern = to_matchit_pattern(&pattern);
        let bucket = inner.method_index.bucket_mut(&method);
        if bucket.trie.insert(matchit_pattern.clone(), idx).is_err() {
            // Pattern conflicts with an already-registered one (e.g. two routes
            // collapse to the same matchit shape). Keep it in the linear
            // fallback so registration never silently drops a route.
            bucket.fallback.push(idx);
        }

        // Mirror the pattern into the unified path-existence trie. We only
        // attempt the insert if we haven't seen this exact pattern before,
        // since matchit can't distinguish "duplicate" from "conflict".
        if inner.any_method_seen.insert(matchit_pattern.clone())
            && inner.any_method_trie.insert(matchit_pattern, ()).is_err()
        {
            // Genuine structural conflict — fall back to the per-bucket
            // walk for miss detection so we don't false-negative.
            inner.any_method_has_fallback = true;
        }

        inner.routes.push(RouteEntry {
            method,
            pattern,
            match_fn,
            handler,
        });
    }

    /// Set the state injector function.
    pub(crate) fn set_state_injector(
        &self,
        injector: Arc<dyn Fn(&mut http::Extensions) + Send + Sync>,
    ) {
        self.inner.write().state_injector = Some(injector);
    }

    /// Set a fallback service for requests that don't match any typeway route.
    pub(crate) fn set_fallback(&self, fallback: FallbackService) {
        self.inner.write().fallback = Some(fallback);
    }

    /// Look up a handler by HTTP method and route pattern string.
    ///
    /// Returns a clone of the handler if found. Since `BoxedHandler` is
    /// `Arc`-wrapped, this clone is cheap (reference count increment).
    ///
    /// Used by the native gRPC server to build its own dispatch table
    /// from the already-registered REST handlers.
    #[cfg(feature = "grpc")]
    pub(crate) fn find_handler_by_pattern(
        &self,
        method: &http::Method,
        pattern: &str,
    ) -> Option<BoxedHandler> {
        let inner = self.inner.read();
        inner
            .routes
            .iter()
            .find(|e| e.method == *method && e.pattern == pattern)
            .map(|e| e.handler.clone())
    }

    /// Get a clone of the state injector, if one is set.
    #[cfg(feature = "grpc")]
    pub(crate) fn state_injector(&self) -> Option<StateInjector> {
        self.inner.read().state_injector.clone()
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
        let inner = self.inner.read();

        // Consume req into parts up front so we can borrow path from parts.uri
        // without conflicting with the later move into the handler. If we hit
        // a fallback path we reassemble the request below.
        let (mut parts, body) = req.into_parts();

        let path: &str = parts.uri.path();
        let lookup_path = match strip_prefix(inner.prefix_str.as_deref(), path) {
            Some(p) => p,
            None => {
                // Path doesn't fall under the configured prefix.
                return if let Some(ref fallback) = inner.fallback {
                    fallback(http::Request::from_parts(parts, body))
                } else {
                    not_found_response()
                };
            }
        };

        match resolve(&inner, &parts.method, lookup_path) {
            LookupOutcome::Hit(i) => {
                // Tell `Path<T>` how many bytes of the URI path are prefix.
                // Skipped when there's no prefix to keep the common path allocation-free.
                if let Some(ref prefix_str) = inner.prefix_str {
                    parts.extensions.insert(PathPrefixOffset(prefix_str.len()));
                }
                if let Some(ref injector) = inner.state_injector {
                    injector(&mut parts.extensions);
                }
                let router = self.clone();
                let max_body = inner.max_body_size;
                drop(inner);
                Box::pin(async move {
                    let body_bytes = match collect_body_limited(body, max_body).await {
                        Ok(bytes) => bytes,
                        Err(resp) => return resp,
                    };
                    let fut = {
                        let inner = router.inner.read();
                        (inner.routes[i].handler)(parts, body_bytes)
                    };
                    fut.await
                })
            }
            LookupOutcome::MethodNotAllowed => method_not_allowed_response(),
            LookupOutcome::NotFound => {
                if let Some(ref fallback) = inner.fallback {
                    fallback(http::Request::from_parts(parts, body))
                } else {
                    not_found_response()
                }
            }
        }
    }

    /// Route a request whose body has already been collected into [`bytes::Bytes`].
    ///
    /// Used by adapters that have a different body type from `hyper::body::Incoming`
    /// (e.g. the Axum interop layer), and by anything that has already buffered
    /// a body for an unrelated reason. Bypasses the `max_body_size` check;
    /// the caller is responsible for any size limiting.
    pub fn route_with_bytes(
        self: &Arc<Self>,
        mut parts: http::request::Parts,
        body_bytes: bytes::Bytes,
    ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
        let inner = self.inner.read();

        let path: &str = parts.uri.path();
        let lookup_path = match strip_prefix(inner.prefix_str.as_deref(), path) {
            Some(p) => p,
            None => return not_found_response(),
        };

        match resolve(&inner, &parts.method, lookup_path) {
            LookupOutcome::Hit(i) => {
                if let Some(ref prefix_str) = inner.prefix_str {
                    parts.extensions.insert(PathPrefixOffset(prefix_str.len()));
                }
                if let Some(ref injector) = inner.state_injector {
                    injector(&mut parts.extensions);
                }
                let fut = (inner.routes[i].handler)(parts, body_bytes);
                drop(inner);
                fut
            }
            LookupOutcome::MethodNotAllowed => method_not_allowed_response(),
            LookupOutcome::NotFound => not_found_response(),
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
/// timeouts, etc.) to the typeway router.
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
        Box::pin(async move {
            use futures::FutureExt;

            let result = std::panic::AssertUnwindSafe(router.route(req))
                .catch_unwind()
                .await;

            match result {
                Ok(response) => Ok(response),
                Err(panic_info) => {
                    let message = if let Some(s) = panic_info.downcast_ref::<&str>() {
                        (*s).to_string()
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };

                    tracing::error!("handler panicked: {message}");

                    let mut res =
                        http::Response::new(body_from_string("Internal Server Error".to_string()));
                    *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    Ok(res)
                }
            }
        })
    }
}
