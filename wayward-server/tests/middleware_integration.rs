//! Integration tests for Tower middleware support.

use std::time::Duration;

use wayward_core::*;
use wayward_macros::*;
use wayward_server::*;

use tower_http::cors::{Any, CorsLayer};
use tower_http::timeout::TimeoutLayer;

wayward_path!(type HelloPath = "hello");
wayward_path!(type SlowPath = "slow");

type API = (
    GetEndpoint<HelloPath, String>,
    GetEndpoint<SlowPath, String>,
);

async fn hello() -> &'static str {
    "Hello!"
}

async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_secs(10)).await;
    "done"
}

async fn start_server_with_layers() -> u16 {
    let server = Server::<API>::new((bind::<_, _, _>(hello), bind::<_, _, _>(slow)));

    let layered =
        server
            .layer(CorsLayer::new().allow_origin(Any))
            .layer(TimeoutLayer::with_status_code(
                http::StatusCode::REQUEST_TIMEOUT,
                Duration::from_millis(500),
            ));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        // Use into_parts to get the inner service for manual serving
        let svc = layered.service;
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
async fn cors_headers_are_set() {
    let port = start_server_with_layers().await;

    let resp = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/hello"))
        .header("Origin", "http://example.com")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers().get("access-control-allow-origin").is_some(),
        "CORS header should be present"
    );
}

#[tokio::test]
async fn timeout_returns_408() {
    let port = start_server_with_layers().await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/slow"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 408, "slow endpoint should timeout with 408");
}

#[tokio::test]
async fn normal_requests_still_work() {
    let port = start_server_with_layers().await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Hello!");
}
