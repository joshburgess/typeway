//! Benchmarks for typeway routing, handler dispatch, and comparison with axum.
//!
//! Three rows are reported per scenario, all driven through a Tower
//! `Service::call`:
//!
//! - `axum/...`: `axum::Router` directly. Baseline.
//! - `typeway/...`: `Router` wrapped in a Tower service that takes
//!   `Request<axum::body::Body>` (the same wire shape axum's bench uses).
//!   This is the apples-to-apples comparison: same Service trait dispatch,
//!   same body type, same matchit matcher under the hood.
//! - `typeway_in_axum/...`: typeway's router nested inside `axum::Router` via
//!   `into_axum_router()` (which uses `fallback_service`). Every request walks
//!   axum's outer router AND typeway's inner router, so this is strictly more
//!   work than either (and shows the cost of nesting).

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use http::Request;
use http_body_util::BodyExt;
use tower::ServiceExt;
use tower_service::Service;

use typeway::prelude::*;
use typeway_server::BoxBody;

// Tower-service wrapper around `Arc<Router>` that takes `axum::body::Body`,
// mirroring axum's per-request shape. This is what makes the `typeway/...`
// row directly comparable to the `axum/...` row.
#[derive(Clone)]
struct TypewayService {
    router: Arc<Router>,
}

impl Service<Request<axum::body::Body>> for TypewayService {
    type Response = http::Response<BoxBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<axum::body::Body>) -> Self::Future {
        let router = self.router.clone();
        Box::pin(async move {
            let (parts, body) = req.into_parts();
            let body_bytes = body
                .collect()
                .await
                .map(|c| c.to_bytes())
                .unwrap_or_default();
            Ok(router.route_with_bytes(parts, body_bytes).await)
        })
    }
}

// ---------------------------------------------------------------------------
// Typeway setup
// ---------------------------------------------------------------------------

typeway_path!(type HelloPath = "hello");
typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);
typeway_path!(type UserPostsPath = "users" / u32 / "posts" / u32);
typeway_path!(type P5 = "a" / u32 / "b" / u32 / "c");
typeway_path!(type P6 = "x" / "y" / "z");
typeway_path!(type P7 = "api" / "v1" / "items" / u32);
typeway_path!(type P8 = "api" / "v1" / "items");
typeway_path!(type P9 = "api" / "v2" / "things" / u32);
typeway_path!(type P10 = "health");

async fn noop() -> &'static str {
    "ok"
}
async fn noop_path(_p: Path<UserByIdPath>) -> &'static str {
    "ok"
}
async fn noop_path2(_p: Path<UserPostsPath>) -> &'static str {
    "ok"
}
async fn noop_p5(_p: Path<P5>) -> &'static str {
    "ok"
}
async fn noop_p7(_p: Path<P7>) -> &'static str {
    "ok"
}
async fn noop_p9(_p: Path<P9>) -> &'static str {
    "ok"
}

// 1-route API (reserved for future single-route benchmarks)
#[allow(dead_code)]
type Api1 = (GetEndpoint<HelloPath, String>,);

// 10-route API
type Api10 = (
    GetEndpoint<HelloPath, String>,
    GetEndpoint<UsersPath, String>,
    GetEndpoint<UserByIdPath, String>,
    GetEndpoint<UserPostsPath, String>,
    GetEndpoint<P5, String>,
    GetEndpoint<P6, String>,
    GetEndpoint<P7, String>,
    GetEndpoint<P8, String>,
    GetEndpoint<P9, String>,
    GetEndpoint<P10, String>,
);

fn make_typeway_server_10() -> Server<Api10> {
    Server::<Api10>::new((
        bind::<_, _, _>(noop),
        bind::<_, _, _>(noop),
        bind::<_, _, _>(noop_path),
        bind::<_, _, _>(noop_path2),
        bind::<_, _, _>(noop_p5),
        bind::<_, _, _>(noop),
        bind::<_, _, _>(noop_p7),
        bind::<_, _, _>(noop),
        bind::<_, _, _>(noop_p9),
        bind::<_, _, _>(noop),
    ))
}

fn make_typeway_tower_10() -> TypewayService {
    TypewayService {
        router: Arc::new(make_typeway_server_10().into_router()),
    }
}

fn make_typeway_in_axum_10() -> axum::Router {
    make_typeway_server_10().into_axum_router()
}

// ---------------------------------------------------------------------------
// Axum setup
// ---------------------------------------------------------------------------

fn make_axum_10() -> axum::Router {
    axum::Router::new()
        .route("/hello", axum::routing::get(|| async { "ok" }))
        .route("/users", axum::routing::get(|| async { "ok" }))
        .route("/users/{id}", axum::routing::get(|| async { "ok" }))
        .route(
            "/users/{id}/posts/{post_id}",
            axum::routing::get(|| async { "ok" }),
        )
        .route("/a/{x}/b/{y}/c", axum::routing::get(|| async { "ok" }))
        .route("/x/y/z", axum::routing::get(|| async { "ok" }))
        .route("/api/v1/items/{id}", axum::routing::get(|| async { "ok" }))
        .route("/api/v1/items", axum::routing::get(|| async { "ok" }))
        .route("/api/v2/things/{id}", axum::routing::get(|| async { "ok" }))
        .route("/health", axum::routing::get(|| async { "ok" }))
}

fn axum_get(path: &str) -> Request<axum::body::Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(axum::body::Body::empty())
        .unwrap()
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_route_matching(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("route_matching");

    // --- Axum 10 routes ---

    let axum10 = make_axum_10();

    group.bench_function("axum/10_routes/first", |b| {
        b.to_async(&rt).iter(|| {
            let svc = axum10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/hello")).await.unwrap();
                black_box(resp);
            }
        });
    });

    group.bench_function("axum/10_routes/last", |b| {
        b.to_async(&rt).iter(|| {
            let svc = axum10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/health")).await.unwrap();
                black_box(resp);
            }
        });
    });

    group.bench_function("axum/10_routes/capture", |b| {
        b.to_async(&rt).iter(|| {
            let svc = axum10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/users/42")).await.unwrap();
                black_box(resp);
            }
        });
    });

    group.bench_function("axum/10_routes/miss", |b| {
        b.to_async(&rt).iter(|| {
            let svc = axum10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/nonexistent")).await.unwrap();
                black_box(resp);
            }
        });
    });

    // --- Typeway 10 routes (bare Tower service, apples-to-apples with axum) ---

    let typeway10 = make_typeway_tower_10();

    group.bench_function("typeway/10_routes/first", |b| {
        b.to_async(&rt).iter(|| {
            let svc = typeway10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/hello")).await.unwrap();
                black_box(resp);
            }
        });
    });

    group.bench_function("typeway/10_routes/last", |b| {
        b.to_async(&rt).iter(|| {
            let svc = typeway10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/health")).await.unwrap();
                black_box(resp);
            }
        });
    });

    group.bench_function("typeway/10_routes/capture", |b| {
        b.to_async(&rt).iter(|| {
            let svc = typeway10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/users/42")).await.unwrap();
                black_box(resp);
            }
        });
    });

    group.bench_function("typeway/10_routes/miss", |b| {
        b.to_async(&rt).iter(|| {
            let svc = typeway10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/nonexistent")).await.unwrap();
                black_box(resp);
            }
        });
    });

    // --- Typeway 10 routes nested inside axum::Router via into_axum_router ---
    //
    // Strictly more work: every request walks axum's outer router AND
    // typeway's inner router. Useful as the upper bound of nesting cost.

    let typeway_in_axum10 = make_typeway_in_axum_10();

    group.bench_function("typeway_in_axum/10_routes/first", |b| {
        b.to_async(&rt).iter(|| {
            let svc = typeway_in_axum10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/hello")).await.unwrap();
                black_box(resp);
            }
        });
    });

    group.bench_function("typeway_in_axum/10_routes/last", |b| {
        b.to_async(&rt).iter(|| {
            let svc = typeway_in_axum10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/health")).await.unwrap();
                black_box(resp);
            }
        });
    });

    group.bench_function("typeway_in_axum/10_routes/capture", |b| {
        b.to_async(&rt).iter(|| {
            let svc = typeway_in_axum10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/users/42")).await.unwrap();
                black_box(resp);
            }
        });
    });

    group.bench_function("typeway_in_axum/10_routes/miss", |b| {
        b.to_async(&rt).iter(|| {
            let svc = typeway_in_axum10.clone();
            async move {
                let resp = svc.oneshot(axum_get("/nonexistent")).await.unwrap();
                black_box(resp);
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_route_matching);
criterion_main!(benches);
