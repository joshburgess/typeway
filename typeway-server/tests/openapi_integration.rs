//! Integration test: verify the server serves /openapi.json and /docs.
//!
//! Run: cargo test -p typeway-server --features openapi --test openapi_integration

#![cfg(feature = "openapi")]

use std::sync::Arc;

use typeway_core::*;
use typeway_macros::*;
use typeway_openapi::*;
use typeway_server::*;

// --- Domain types with ToSchema impls ---

struct User;
impl ToSchema for User {
    fn schema() -> typeway_openapi::spec::Schema {
        typeway_openapi::spec::Schema::object()
    }
    fn type_name() -> &'static str {
        "User"
    }
}

// --- API definition ---

typeway_path!(type HelloPath = "hello");

type API = (GetEndpoint<HelloPath, User>,);

async fn hello() -> &'static str {
    "hello"
}

// --- Test helpers ---

async fn start_server_with_openapi() -> u16 {
    let server = Server::<API>::new((bind::<_, _, _>(hello),)).with_openapi("Test API", "1.0.0");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let router = Arc::new(server.into_router());
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = hyper_util::rt::TokioIo::new(stream);
            let router = router.clone();
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req| {
                    let router = router.clone();
                    async move { Ok::<_, std::convert::Infallible>(router.route(req).await) }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    port
}

// --- Tests ---

#[tokio::test]
async fn openapi_json_is_served() {
    let port = start_server_with_openapi().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/openapi.json"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["openapi"], "3.1.0");
    assert_eq!(body["info"]["title"], "Test API");
    assert_eq!(body["info"]["version"], "1.0.0");
    assert!(body["paths"]["/hello"]["get"].is_object());
}

#[tokio::test]
async fn docs_page_is_served() {
    let port = start_server_with_openapi().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/docs"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );

    let body = resp.text().await.unwrap();
    assert!(body.contains("API Documentation"));
    assert!(body.contains("Test API"));
    assert!(body.contains("/hello"));
}

#[tokio::test]
async fn normal_routes_still_work() {
    let port = start_server_with_openapi().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "hello");
}
