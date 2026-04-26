//! Integration tests for handler panic safety.
//!
//! Verifies that a panicking handler returns 500 Internal Server Error
//! instead of crashing the connection task.

use std::sync::Arc;
use std::time::Duration;

use typeway_core::*;
use typeway_macros::*;
use typeway_server::*;

typeway_path!(type PanicPath = "panic");
typeway_path!(type OkPath = "ok");

type PanicAPI = (GetEndpoint<PanicPath, String>, GetEndpoint<OkPath, String>);

async fn panicking_handler() -> String {
    panic!("handler exploded");
}

async fn ok_handler() -> String {
    "ok".to_string()
}

async fn start_panic_server() -> u16 {
    let server = Server::<PanicAPI>::new((
        bind::<_, _, _>(panicking_handler),
        bind::<_, _, _>(ok_handler),
    ));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let router = Arc::new(server.into_router());
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = RouterService::new(router.clone());
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
async fn panicking_handler_returns_500() {
    let port = start_panic_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/panic"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 500);
    assert_eq!(resp.text().await.unwrap(), "Internal Server Error");
}

#[tokio::test]
async fn server_still_works_after_panic() {
    let port = start_panic_server().await;

    // First request panics but returns 500.
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/panic"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 500);

    // Second request to a non-panicking handler still works.
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/ok"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
}
