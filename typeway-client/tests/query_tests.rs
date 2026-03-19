use std::sync::Arc;
use std::time::Duration;

use http::StatusCode;
use serde::{Deserialize, Serialize};

use typeway_client::{Client, ClientConfig, RetryPolicy};
use typeway_core::*;
use typeway_macros::*;

// --- Types shared by test endpoints ---

typeway_path!(type UsersPath = "users");
type ListUsersEndpoint = GetEndpoint<UsersPath, Vec<TestUser>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestUser {
    id: u32,
    name: String,
}

#[derive(Debug, Serialize)]
struct Pagination {
    page: u32,
    limit: u32,
}

// ---------------------------------------------------------------------------
// Minimal test HTTP server using hyper directly
// ---------------------------------------------------------------------------

async fn start_mock_server<F, Fut>(handler: F) -> u16
where
    F: Fn(hyper::Request<hyper::body::Incoming>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = hyper::Response<http_body_util::Full<bytes::Bytes>>>
        + Send
        + 'static,
{
    let handler = Arc::new(handler);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let io = hyper_util::rt::TokioIo::new(stream);
            let handler = handler.clone();
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req| {
                    let handler = handler.clone();
                    async move { Ok::<_, std::convert::Infallible>(handler(req).await) }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(20)).await;
    port
}

fn json_response(
    status: StatusCode,
    body: &str,
) -> hyper::Response<http_body_util::Full<bytes::Bytes>> {
    hyper::Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(http_body_util::Full::new(bytes::Bytes::from(
            body.to_owned(),
        )))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `call_with_query` appends query parameters to the request URL.
#[tokio::test]
async fn test_call_with_query_appends_params() {
    let received_query = Arc::new(std::sync::Mutex::new(None::<String>));
    let received_query_clone = received_query.clone();

    let port = start_mock_server(move |req| {
        let query = req.uri().query().map(|q| q.to_string());
        *received_query_clone.lock().unwrap() = query;
        async { json_response(StatusCode::OK, "[]") }
    })
    .await;

    let config = ClientConfig::default()
        .no_timeout()
        .retry_policy(RetryPolicy::none());

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();

    let _users: Vec<TestUser> = client
        .call_with_query::<ListUsersEndpoint, _>((), &Pagination { page: 2, limit: 20 })
        .await
        .unwrap();

    let query = received_query.lock().unwrap().clone();
    let query = query.expect("query string should be present");
    assert!(
        query.contains("page=2"),
        "expected page=2 in query string, got: {query}"
    );
    assert!(
        query.contains("limit=20"),
        "expected limit=20 in query string, got: {query}"
    );
}

/// `call_with_query` with an empty struct results in no query string.
#[tokio::test]
async fn test_call_with_query_empty_struct() {
    let received_query = Arc::new(std::sync::Mutex::new(None::<String>));
    let received_query_clone = received_query.clone();

    let port = start_mock_server(move |req| {
        let query = req.uri().query().map(|q| q.to_string());
        *received_query_clone.lock().unwrap() = query;
        async { json_response(StatusCode::OK, "[]") }
    })
    .await;

    let config = ClientConfig::default()
        .no_timeout()
        .retry_policy(RetryPolicy::none());

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();

    #[derive(Serialize)]
    struct Empty {}

    let _: Vec<TestUser> = client
        .call_with_query::<ListUsersEndpoint, _>((), &Empty {})
        .await
        .unwrap();

    // Empty struct serializes to "" which is skipped.
    let query = received_query.lock().unwrap().clone();
    assert!(
        query.is_none(),
        "expected no query string for empty struct, got: {query:?}"
    );
}

/// Default `Accept: application/json` header is sent on every request.
#[tokio::test]
async fn test_accept_header_is_present() {
    let received_accept = Arc::new(std::sync::Mutex::new(None::<String>));
    let received_accept_clone = received_accept.clone();

    let port = start_mock_server(move |req| {
        let accept = req
            .headers()
            .get("accept")
            .map(|v| v.to_str().unwrap().to_string());
        *received_accept_clone.lock().unwrap() = accept;
        async { json_response(StatusCode::OK, "[]") }
    })
    .await;

    let config = ClientConfig::default()
        .no_timeout()
        .retry_policy(RetryPolicy::none());

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    let _: Vec<TestUser> = client.call::<ListUsersEndpoint>(()).await.unwrap();

    let accept = received_accept.lock().unwrap().clone();
    assert_eq!(accept, Some("application/json".to_string()));
}

/// `enable_tracing` compiles and does not panic.
#[tokio::test]
async fn test_enable_tracing_does_not_panic() {
    let port = start_mock_server(move |_req| async {
        json_response(StatusCode::OK, "[]")
    })
    .await;

    let config = ClientConfig::default()
        .no_timeout()
        .retry_policy(RetryPolicy::none())
        .enable_tracing();

    assert!(config.enable_tracing);

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    let _: Vec<TestUser> = client.call::<ListUsersEndpoint>(()).await.unwrap();
}

/// `with_tracing` convenience function enables tracing on a config.
#[test]
fn test_with_tracing_convenience() {
    let config = typeway_client::with_tracing(ClientConfig::default());
    assert!(config.enable_tracing);
}
