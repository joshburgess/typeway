//! Integration tests for the SecureHeadersLayer middleware.

use std::time::Duration;

use typeway_core::*;
use typeway_macros::*;
use typeway_server::*;

typeway_path!(type HelloPath = "hello");

type API = (GetEndpoint<HelloPath, String>,);

async fn hello() -> &'static str {
    "Hello!"
}

/// Start a server with the given SecureHeadersLayer and return the port.
async fn start_server(layer: SecureHeadersLayer) -> u16 {
    let server = Server::<API>::new((bind::<_, _, _>(hello),));
    let layered = server.layer(layer);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
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
async fn default_security_headers_are_set() {
    let port = start_server(SecureHeadersLayer::new()).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Hello!");
}

#[tokio::test]
async fn all_default_headers_present() {
    let port = start_server(SecureHeadersLayer::new()).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();

    let headers = resp.headers();

    assert_eq!(
        headers
            .get("x-content-type-options")
            .unwrap()
            .to_str()
            .unwrap(),
        "nosniff"
    );
    assert_eq!(
        headers.get("x-frame-options").unwrap().to_str().unwrap(),
        "DENY"
    );
    assert_eq!(
        headers.get("x-xss-protection").unwrap().to_str().unwrap(),
        "0"
    );
    assert_eq!(
        headers.get("referrer-policy").unwrap().to_str().unwrap(),
        "strict-origin-when-cross-origin"
    );
    assert_eq!(
        headers
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap(),
        "default-src 'self'"
    );
    assert_eq!(
        headers.get("permissions-policy").unwrap().to_str().unwrap(),
        "camera=(), microphone=(), geolocation=()"
    );

    // HSTS should NOT be present by default.
    assert!(
        headers.get("strict-transport-security").is_none(),
        "HSTS should not be set by default"
    );
}

#[tokio::test]
async fn hsts_adds_strict_transport_security() {
    let layer = SecureHeadersLayer::new().hsts(63_072_000);
    let port = start_server(layer).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();

    let hsts = resp
        .headers()
        .get("strict-transport-security")
        .expect("HSTS header should be present")
        .to_str()
        .unwrap();

    assert_eq!(hsts, "max-age=63072000; includeSubDomains; preload");
}

#[tokio::test]
async fn disable_csp_removes_header() {
    let layer = SecureHeadersLayer::new().disable_csp();
    let port = start_server(layer).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();

    assert!(
        resp.headers().get("content-security-policy").is_none(),
        "CSP header should be removed when disabled"
    );

    // Other headers should still be present.
    assert!(resp.headers().get("x-content-type-options").is_some());
    assert!(resp.headers().get("x-frame-options").is_some());
}

#[tokio::test]
async fn frame_options_override() {
    let layer = SecureHeadersLayer::new().frame_options("SAMEORIGIN");
    let port = start_server(layer).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();

    assert_eq!(
        resp.headers()
            .get("x-frame-options")
            .unwrap()
            .to_str()
            .unwrap(),
        "SAMEORIGIN"
    );
}

#[tokio::test]
async fn custom_header_added() {
    let layer = SecureHeadersLayer::new().custom("x-custom-header", "custom-value");
    let port = start_server(layer).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();

    assert_eq!(
        resp.headers()
            .get("x-custom-header")
            .unwrap()
            .to_str()
            .unwrap(),
        "custom-value"
    );
}

#[tokio::test]
async fn content_security_policy_override() {
    let layer = SecureHeadersLayer::new().content_security_policy("default-src 'self'; img-src *");
    let port = start_server(layer).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();

    assert_eq!(
        resp.headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap(),
        "default-src 'self'; img-src *"
    );
}
