//! Adversarial request body size limit tests.
//!
//! These tests verify that the body size limit enforcement in
//! `collect_body_limited` is robust against edge cases: exact
//! boundaries, off-by-one, custom limits, empty bodies, misleading
//! Content-Length headers, and chunked transfer encoding.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use typeway_core::*;
use typeway_macros::*;
use typeway_server::router::DEFAULT_MAX_BODY_SIZE;
use typeway_server::*;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

typeway_path!(type EchoPath = "echo");
typeway_path!(type HelloPath = "hello");

type EchoAPI = (PostEndpoint<EchoPath, String, String>,);

async fn echo_body(body: String) -> String {
    body
}

/// Spawn a hyper server from a `Router` and return the port.
async fn spawn_router(router: Router) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = Arc::new(router);

    tokio::spawn(async move {
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

/// Start a server with the default 2 MiB body limit.
async fn start_default_server() -> u16 {
    let server = Server::<EchoAPI>::new((bind::<_, _, _>(echo_body),));
    spawn_router(server.into_router()).await
}

/// Start a server with a custom body limit.
async fn start_custom_limit_server(max: usize) -> u16 {
    let server = Server::<EchoAPI>::new((bind::<_, _, _>(echo_body),)).max_body_size(max);
    spawn_router(server.into_router()).await
}

type GetAndPostAPI = (
    GetEndpoint<HelloPath, String>,
    PostEndpoint<EchoPath, String, String>,
);

async fn hello() -> &'static str {
    "hello"
}

/// Start a server with both GET and POST routes and a custom body limit.
async fn start_get_post_server(max: usize) -> u16 {
    let server = Server::<GetAndPostAPI>::new((
        bind::<_, _, _>(hello),
        bind::<_, _, _>(echo_body),
    ))
    .max_body_size(max);
    spawn_router(server.into_router()).await
}

/// Send a raw HTTP/1.1 request over TCP and return the full response bytes.
///
/// Includes `Connection: close` so the server closes its end after
/// responding, allowing `read_to_end` to complete.
async fn send_raw(port: u16, request: &[u8]) -> String {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    stream.write_all(request).await.unwrap();

    // Give the server time to process the request before reading.
    // We do NOT shutdown the write half immediately — the server may
    // still be reading when hyper processes chunked encoding.
    let mut buf = vec![0u8; 16384];
    let mut response = Vec::new();

    // Read with a timeout to avoid hanging forever.
    loop {
        match tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf)).await {
            Ok(Ok(0)) => break,             // EOF
            Ok(Ok(n)) => response.extend_from_slice(&buf[..n]),
            Ok(Err(_)) => break,            // read error
            Err(_) => break,                // timeout
        }
    }

    String::from_utf8_lossy(&response).to_string()
}

/// Extract the HTTP status code from a raw response string.
fn extract_status(raw: &str) -> u16 {
    // Expect "HTTP/1.1 NNN ..."
    let status_str = raw
        .split_whitespace()
        .nth(1)
        .expect("missing status code in response");
    status_str.parse().expect("non-numeric status code")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// 1. A body exactly at the default max size (2 MiB) should succeed.
#[tokio::test]
async fn exact_limit_boundary_succeeds() {
    let port = start_default_server().await;
    let body = "x".repeat(DEFAULT_MAX_BODY_SIZE);

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/echo"))
        .body(body.clone())
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200, "body exactly at limit should succeed");
    assert_eq!(resp.text().await.unwrap(), body);
}

/// 2. A body one byte over the default max size should be rejected as 413.
#[tokio::test]
async fn one_byte_over_limit_returns_413() {
    let port = start_default_server().await;
    let body = "x".repeat(DEFAULT_MAX_BODY_SIZE + 1);

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/echo"))
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        413,
        "body one byte over limit should be rejected"
    );
}

/// 3. A custom small limit rejects bodies above it.
#[tokio::test]
async fn custom_limit_enforcement() {
    let port = start_custom_limit_server(1024).await;
    let body = "x".repeat(1025);

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/echo"))
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        413,
        "1025 bytes should exceed custom 1024-byte limit"
    );
}

/// 3b. A body exactly at a custom limit should succeed.
#[tokio::test]
async fn custom_limit_exact_boundary_succeeds() {
    let port = start_custom_limit_server(1024).await;
    let body = "x".repeat(1024);

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/echo"))
        .body(body.clone())
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        200,
        "body exactly at custom limit should succeed"
    );
    assert_eq!(resp.text().await.unwrap(), body);
}

/// 4. An empty body on a POST endpoint expecting a string should return 400
///    (the handler receives an empty string which is valid for `String`
///    extraction, but the test documents the behavior — it should not crash).
#[tokio::test]
async fn zero_byte_body_on_post_does_not_crash() {
    let port = start_default_server().await;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/echo"))
        .header("content-type", "application/json")
        .body("")
        .send()
        .await
        .unwrap();

    // An empty body sent to a String extractor should either succeed (empty
    // string) or fail with 400 — either is acceptable; a panic/5xx is not.
    let status = resp.status().as_u16();
    assert!(
        status < 500,
        "empty body should not cause a server error, got {status}"
    );
}

/// 5. A request claiming a very large Content-Length but sending only a few
///    bytes should not cause the server to allocate based on Content-Length
///    alone. The connection may close or the server may respond, but it must
///    not OOM.
#[tokio::test]
async fn large_content_length_header_does_not_allocate() {
    let port = start_custom_limit_server(1024).await;

    // Claim 1 GB but only send 5 bytes.
    let request = format!(
        "POST /echo HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Connection: close\r\n\
         Content-Length: 1073741824\r\n\
         \r\n\
         hello"
    );

    let raw = send_raw(port, request.as_bytes()).await;

    // The server should either:
    // - Reject with 413 (it sees Content-Length > limit), or
    // - Close the connection, or
    // - Eventually time out waiting for more data.
    // Any of these is acceptable. A 200 with successful allocation is not.
    if !raw.is_empty() {
        let status = extract_status(&raw);
        assert_ne!(
            status, 200,
            "server should not accept a request claiming 1 GB body"
        );
    }
    // If raw is empty, the server closed the connection, which is also fine.
}

/// 6. Data sent in many small chunks via chunked transfer encoding that
///    together exceed the limit should be rejected with 413.
#[tokio::test]
async fn chunked_encoding_exceeding_limit_returns_413() {
    let port = start_custom_limit_server(64).await;

    // Build a chunked request with 10 chunks of 10 bytes each = 100 bytes > 64 limit.
    let chunk = "a".repeat(10);
    let mut body = String::new();
    for _ in 0..10 {
        body.push_str(&format!("{:x}\r\n{}\r\n", chunk.len(), chunk));
    }
    body.push_str("0\r\n\r\n");

    let request = format!(
        "POST /echo HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Connection: close\r\n\
         Transfer-Encoding: chunked\r\n\
         \r\n\
         {body}"
    );

    let raw = send_raw(port, request.as_bytes()).await;
    assert!(!raw.is_empty(), "server should respond, not drop connection");
    let status = extract_status(&raw);
    assert_eq!(
        status, 413,
        "chunked body exceeding limit should be rejected with 413"
    );
}

/// 6b. Chunked data within the limit should succeed.
#[tokio::test]
async fn chunked_encoding_within_limit_succeeds() {
    let port = start_custom_limit_server(256).await;

    // 5 chunks of 10 bytes = 50 bytes, well within 256 limit.
    let chunk = "b".repeat(10);
    let mut body = String::new();
    for _ in 0..5 {
        body.push_str(&format!("{:x}\r\n{}\r\n", chunk.len(), chunk));
    }
    body.push_str("0\r\n\r\n");

    let request = format!(
        "POST /echo HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Connection: close\r\n\
         Transfer-Encoding: chunked\r\n\
         \r\n\
         {body}"
    );

    let raw = send_raw(port, request.as_bytes()).await;
    assert!(!raw.is_empty(), "server should respond");
    let status = extract_status(&raw);
    assert_eq!(
        status, 200,
        "chunked body within limit should succeed, got {status}"
    );
}

/// 7. GET requests with no body should work fine even when max_body_size
///    is set to a very small value.
#[tokio::test]
async fn get_with_small_body_limit_works() {
    let port = start_get_post_server(1).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "hello");
}
