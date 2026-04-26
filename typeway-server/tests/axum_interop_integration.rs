//! Integration test: typeway API nested inside an Axum app.
//!
//! Run: cargo test -p typeway-server --features axum-interop --test axum_interop_integration

#![cfg(feature = "axum-interop")]

use std::time::Duration;

use typeway_core::*;
use typeway_macros::*;
use typeway_server::*;

typeway_path!(type HelloPath = "hello");

type API = (GetEndpoint<HelloPath, String>,);

async fn hello() -> &'static str {
    "Hello from Typeway!"
}

async fn start_mixed_server() -> u16 {
    let typeway_api = Server::<API>::new((bind::<_, _, _>(hello),));

    // Build an Axum app with typeway nested at /api
    let app = axum::Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .nest("/api", typeway_api.into_axum_router());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

#[tokio::test]
async fn axum_native_route_works() {
    let port = start_mixed_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/health"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
}

#[tokio::test]
async fn typeway_nested_route_works() {
    let port = start_mixed_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/api/hello"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Hello from Typeway!");
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let port = start_mixed_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/api/nonexistent"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// --- Reverse direction: Axum fallback inside typeway ---

async fn start_typeway_with_axum_fallback() -> u16 {
    let axum_routes = axum::Router::new()
        .route("/health", axum::routing::get(|| async { "ok from axum" }))
        .route("/info", axum::routing::get(|| async { "axum info" }));

    let server = Server::<API>::new((bind::<_, _, _>(hello),)).with_axum_fallback(axum_routes);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let svc = server.into_service();

    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = svc.clone();
            let hyper_svc = hyper_util::service::TowerToHyperService::new(svc);
            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, hyper_svc)
                    .await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

#[tokio::test]
async fn typeway_route_with_axum_fallback() {
    let port = start_typeway_with_axum_fallback().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Hello from Typeway!");
}

#[tokio::test]
async fn axum_fallback_route_works() {
    let port = start_typeway_with_axum_fallback().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/health"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok from axum");
}

#[tokio::test]
async fn axum_fallback_second_route() {
    let port = start_typeway_with_axum_fallback().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/info"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "axum info");
}
