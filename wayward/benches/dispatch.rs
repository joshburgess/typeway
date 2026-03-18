//! Benchmarks measuring the cost of wayward's type erasure and handler dispatch.
//!
//! These isolate the overhead of:
//! - BoxedHandler indirection (dyn Fn + Box<dyn Future>)
//! - Handler::call (extractor resolution + response conversion)
//! - Extractor overhead (FromRequestParts resolution)
//! - Body collection at the router boundary
//!
//! Each benchmark compares against a "bare" baseline — a direct async fn call
//! with no framework overhead.

use std::sync::Arc;

use bytes::Bytes;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

use wayward_core::*;
use wayward_macros::*;
use wayward_server::extract::PathSegments;
use wayward_server::handler::WithBody;
use wayward_server::*;

// ---------------------------------------------------------------------------
// Test handlers
// ---------------------------------------------------------------------------

async fn bare_noop() -> &'static str {
    "ok"
}

async fn bare_with_work() -> String {
    let mut s = String::with_capacity(64);
    for i in 0..10 {
        s.push_str(&format!("item-{i},"));
    }
    s
}

wayward_path!(type UserByIdPath = "users" / u32);

async fn bare_path_extract(path: Path<UserByIdPath>) -> String {
    let (id,) = path.0;
    format!("user-{id}")
}

#[derive(Clone)]
struct AppState {
    name: String,
}

async fn bare_state_extract(state: State<AppState>) -> String {
    state.0.name.clone()
}

async fn bare_multi_extract(path: Path<UserByIdPath>, state: State<AppState>) -> String {
    let (id,) = path.0;
    format!("{}-{id}", state.0.name)
}

#[derive(serde::Deserialize)]
struct CreateBody {
    name: String,
}

async fn bare_json_body(body: Json<CreateBody>) -> String {
    body.0.name
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build mock request Parts with path segments and state in extensions.
fn mock_parts(path: &str, state: Option<AppState>) -> http::request::Parts {
    let (mut parts, _) = http::Request::builder()
        .uri(path)
        .body(())
        .unwrap()
        .into_parts();

    let segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    parts.extensions.insert(PathSegments(Arc::new(segments)));

    if let Some(s) = state {
        parts.extensions.insert(s);
    }

    parts
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_handler_dispatch(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("handler_dispatch");

    // --- Baseline: direct async fn call (no framework) ---

    group.bench_function("baseline/noop", |b| {
        b.to_async(&rt).iter(|| async {
            let res = bare_noop().await;
            black_box(res);
        });
    });

    group.bench_function("baseline/with_work", |b| {
        b.to_async(&rt).iter(|| async {
            let res = bare_with_work().await;
            black_box(res);
        });
    });

    // --- BoxedHandler: type-erased dispatch (the real cost) ---

    let noop_boxed = into_boxed_handler::<_, ()>(bare_noop);
    group.bench_function("boxed_handler/noop", |b| {
        b.to_async(&rt).iter(|| {
            let parts = mock_parts("/hello", None);
            let bytes = Bytes::new();
            let handler = &noop_boxed;
            async move {
                let res = handler(parts, bytes).await;
                black_box(res);
            }
        });
    });

    let work_boxed = into_boxed_handler::<_, ()>(bare_with_work);
    group.bench_function("boxed_handler/with_work", |b| {
        b.to_async(&rt).iter(|| {
            let parts = mock_parts("/hello", None);
            let bytes = Bytes::new();
            let handler = &work_boxed;
            async move {
                let res = handler(parts, bytes).await;
                black_box(res);
            }
        });
    });

    // --- Extractor cost: Path<P> ---

    let path_boxed = into_boxed_handler::<_, (Path<UserByIdPath>,)>(bare_path_extract);
    group.bench_function("boxed_handler/path_extract", |b| {
        b.to_async(&rt).iter(|| {
            let parts = mock_parts("/users/42", None);
            let bytes = Bytes::new();
            let handler = &path_boxed;
            async move {
                let res = handler(parts, bytes).await;
                black_box(res);
            }
        });
    });

    // --- Extractor cost: State<T> ---

    let state_boxed = into_boxed_handler::<_, (State<AppState>,)>(bare_state_extract);
    let state = AppState {
        name: "test".into(),
    };
    group.bench_function("boxed_handler/state_extract", |b| {
        b.to_async(&rt).iter(|| {
            let parts = mock_parts("/whatever", Some(state.clone()));
            let bytes = Bytes::new();
            let handler = &state_boxed;
            async move {
                let res = handler(parts, bytes).await;
                black_box(res);
            }
        });
    });

    // --- Extractor cost: Path + State (multiple extractors) ---

    let multi_boxed =
        into_boxed_handler::<_, (Path<UserByIdPath>, State<AppState>)>(bare_multi_extract);
    group.bench_function("boxed_handler/multi_extract", |b| {
        b.to_async(&rt).iter(|| {
            let parts = mock_parts("/users/42", Some(state.clone()));
            let bytes = Bytes::new();
            let handler = &multi_boxed;
            async move {
                let res = handler(parts, bytes).await;
                black_box(res);
            }
        });
    });

    // --- Body extractor cost: Json<T> ---

    let json_boxed = into_boxed_handler::<_, WithBody<(), Json<CreateBody>>>(bare_json_body);
    let json_body = Bytes::from(r#"{"name":"Alice"}"#);
    group.bench_function("boxed_handler/json_body", |b| {
        b.to_async(&rt).iter(|| {
            let parts = mock_parts("/items", None);
            let bytes = json_body.clone();
            let handler = &json_boxed;
            async move {
                let res = handler(parts, bytes).await;
                black_box(res);
            }
        });
    });

    group.finish();
}

fn bench_body_collection(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("body_collection");

    // Measure the cost of Bytes::clone for various sizes.
    // In the router, body bytes are pre-collected and passed by value.

    for size in [0, 64, 1024, 16384, 65536] {
        let data = Bytes::from(vec![b'x'; size]);
        group.bench_function(&format!("bytes_clone/{size}B"), |b| {
            b.iter(|| {
                let cloned = data.clone();
                black_box(cloned);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_handler_dispatch, bench_body_collection);
criterion_main!(benches);
