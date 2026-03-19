use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use http::StatusCode;
use serde::{Deserialize, Serialize};

use typeway_client::{Client, ClientConfig, ClientError, RequestInterceptor, ResponseInterceptor, RetryPolicy};
use typeway_core::*;
use typeway_macros::*;

// --- Types shared by test endpoints ---

typeway_path!(type HealthPath = "health");
type HealthEndpoint = GetEndpoint<HealthPath, HealthResponse>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct HealthResponse {
    status: String,
}

// ---------------------------------------------------------------------------
// Minimal test HTTP server using hyper directly
// ---------------------------------------------------------------------------

/// Starts a hyper server that invokes `handler` for every request.
/// Returns the port it is listening on.
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

    // Give the listener a moment to start.
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

/// A request interceptor adds a custom header that the server can observe.
#[tokio::test]
async fn test_request_interceptor_adds_header() {
    let received_header = Arc::new(std::sync::Mutex::new(None::<String>));
    let received_header_clone = received_header.clone();

    let port = start_mock_server(move |req| {
        let value = req
            .headers()
            .get("x-custom-header")
            .map(|v| v.to_str().unwrap().to_string());
        *received_header_clone.lock().unwrap() = value;
        async { json_response(StatusCode::OK, r#"{"status":"ok"}"#) }
    })
    .await;

    let interceptor: RequestInterceptor = Arc::new(|req| req.header("x-custom-header", "hello"));

    let config = ClientConfig::default()
        .no_timeout()
        .retry_policy(RetryPolicy::none())
        .request_interceptor(interceptor);

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    let result = client.call::<HealthEndpoint>(()).await.unwrap();
    assert_eq!(
        result,
        HealthResponse {
            status: "ok".into()
        }
    );

    let header = received_header.lock().unwrap().clone();
    assert_eq!(header, Some("hello".to_string()));
}

/// Multiple request interceptors are applied in order.
#[tokio::test]
async fn test_multiple_request_interceptors() {
    let received_headers = Arc::new(std::sync::Mutex::new(Vec::<(String, String)>::new()));
    let received_clone = received_headers.clone();

    let port = start_mock_server(move |req| {
        let mut headers = Vec::new();
        for (name, value) in req.headers() {
            if name.as_str().starts_with("x-int-") {
                headers.push((
                    name.as_str().to_string(),
                    value.to_str().unwrap().to_string(),
                ));
            }
        }
        *received_clone.lock().unwrap() = headers;
        async { json_response(StatusCode::OK, r#"{"status":"ok"}"#) }
    })
    .await;

    let config = ClientConfig::default()
        .no_timeout()
        .retry_policy(RetryPolicy::none())
        .request_interceptor(Arc::new(|req| req.header("x-int-first", "1")))
        .request_interceptor(Arc::new(|req| req.header("x-int-second", "2")));

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    client.call::<HealthEndpoint>(()).await.unwrap();

    let headers = received_headers.lock().unwrap().clone();
    assert!(headers.iter().any(|(k, v)| k == "x-int-first" && v == "1"));
    assert!(headers
        .iter()
        .any(|(k, v)| k == "x-int-second" && v == "2"));
}

/// A response interceptor is called with the response.
#[tokio::test]
async fn test_response_interceptor_is_called() {
    let call_count = Arc::new(AtomicU32::new(0));
    let interceptor_count = Arc::new(AtomicU32::new(0));
    let interceptor_status = Arc::new(std::sync::Mutex::new(None::<u16>));

    let call_count_clone = call_count.clone();
    let port = start_mock_server(move |_req| {
        call_count_clone.fetch_add(1, Ordering::SeqCst);
        async { json_response(StatusCode::OK, r#"{"status":"ok"}"#) }
    })
    .await;

    let interceptor_count_clone = interceptor_count.clone();
    let interceptor_status_clone = interceptor_status.clone();
    let resp_interceptor: ResponseInterceptor = Arc::new(move |resp| {
        interceptor_count_clone.fetch_add(1, Ordering::SeqCst);
        *interceptor_status_clone.lock().unwrap() = Some(resp.status().as_u16());
    });

    let config = ClientConfig::default()
        .no_timeout()
        .retry_policy(RetryPolicy::none())
        .response_interceptor(resp_interceptor);

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    client.call::<HealthEndpoint>(()).await.unwrap();

    assert_eq!(call_count.load(Ordering::SeqCst), 1);
    assert_eq!(interceptor_count.load(Ordering::SeqCst), 1);
    assert_eq!(*interceptor_status.lock().unwrap(), Some(200));
}

/// Response interceptor is called even on error responses.
#[tokio::test]
async fn test_response_interceptor_called_on_error_status() {
    let interceptor_status = Arc::new(std::sync::Mutex::new(None::<u16>));
    let interceptor_status_clone = interceptor_status.clone();

    let port = start_mock_server(move |_req| async {
        json_response(StatusCode::NOT_FOUND, r#""not found""#)
    })
    .await;

    let resp_interceptor: ResponseInterceptor = Arc::new(move |resp| {
        *interceptor_status_clone.lock().unwrap() = Some(resp.status().as_u16());
    });

    let config = ClientConfig::default()
        .no_timeout()
        .retry_policy(RetryPolicy::none())
        .response_interceptor(resp_interceptor);

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    let err = client.call::<HealthEndpoint>(()).await.unwrap_err();

    match err {
        ClientError::Status { status, .. } => assert_eq!(status, StatusCode::NOT_FOUND),
        other => panic!("expected Status error, got: {other:?}"),
    }

    // Interceptor should still have been called.
    assert_eq!(*interceptor_status.lock().unwrap(), Some(404));
}

/// `bearer_auth` sets the Authorization header.
#[tokio::test]
async fn test_bearer_auth_sets_authorization_header() {
    let received_auth = Arc::new(std::sync::Mutex::new(None::<String>));
    let received_auth_clone = received_auth.clone();

    let port = start_mock_server(move |req| {
        let auth = req
            .headers()
            .get("authorization")
            .map(|v| v.to_str().unwrap().to_string());
        *received_auth_clone.lock().unwrap() = auth;
        async { json_response(StatusCode::OK, r#"{"status":"ok"}"#) }
    })
    .await;

    let config = ClientConfig::default()
        .no_timeout()
        .retry_policy(RetryPolicy::none())
        .bearer_auth("my-secret-token");

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    client.call::<HealthEndpoint>(()).await.unwrap();

    let auth = received_auth.lock().unwrap().clone();
    assert_eq!(auth, Some("Bearer my-secret-token".to_string()));
}

/// `default_header` adds a header to every request.
#[tokio::test]
async fn test_default_header() {
    let received_header = Arc::new(std::sync::Mutex::new(None::<String>));
    let received_header_clone = received_header.clone();

    let port = start_mock_server(move |req| {
        let value = req
            .headers()
            .get("x-api-key")
            .map(|v| v.to_str().unwrap().to_string());
        *received_header_clone.lock().unwrap() = value;
        async { json_response(StatusCode::OK, r#"{"status":"ok"}"#) }
    })
    .await;

    let config = ClientConfig::default()
        .no_timeout()
        .retry_policy(RetryPolicy::none())
        .default_header(
            http::header::HeaderName::from_static("x-api-key"),
            http::header::HeaderValue::from_static("secret-key-123"),
        );

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    client.call::<HealthEndpoint>(()).await.unwrap();

    let header = received_header.lock().unwrap().clone();
    assert_eq!(header, Some("secret-key-123".to_string()));
}

/// Cookie store config construction works correctly.
#[tokio::test]
async fn test_cookie_store_config() {
    let config = ClientConfig::default().cookie_store(true);
    assert!(config.cookie_store);

    let config = ClientConfig::default().cookie_store(false);
    assert!(!config.cookie_store);

    // Default should be false.
    let config = ClientConfig::default();
    assert!(!config.cookie_store);
}

/// Cookie store persists cookies across requests.
#[tokio::test]
async fn test_cookie_store_persists_cookies() {
    let cookie_received = Arc::new(std::sync::Mutex::new(None::<String>));
    let cookie_received_clone = cookie_received.clone();
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let port = start_mock_server(move |req| {
        let count = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
        let cookie = req
            .headers()
            .get("cookie")
            .map(|v| v.to_str().unwrap().to_string());
        *cookie_received_clone.lock().unwrap() = cookie;

        async move {
            if count == 1 {
                // First request: set a cookie.
                hyper::Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/json")
                    .header("set-cookie", "session=abc123; Path=/")
                    .body(http_body_util::Full::new(bytes::Bytes::from(
                        r#"{"status":"ok"}"#,
                    )))
                    .unwrap()
            } else {
                // Subsequent requests: just respond.
                json_response(StatusCode::OK, r#"{"status":"ok"}"#)
            }
        }
    })
    .await;

    let config = ClientConfig::default()
        .no_timeout()
        .retry_policy(RetryPolicy::none())
        .cookie_store(true);

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();

    // First request — sets the cookie.
    client.call::<HealthEndpoint>(()).await.unwrap();
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    // Second request — cookie should be sent back.
    client.call::<HealthEndpoint>(()).await.unwrap();
    assert_eq!(call_count.load(Ordering::SeqCst), 2);

    let cookie = cookie_received.lock().unwrap().clone();
    assert_eq!(cookie, Some("session=abc123".to_string()));
}

/// `ClientConfig` Debug output works (interceptors are not printed as closures).
#[test]
fn test_client_config_debug() {
    let config = ClientConfig::default()
        .request_interceptor(Arc::new(|req| req))
        .response_interceptor(Arc::new(|_resp| {}));

    let debug = format!("{config:?}");
    assert!(debug.contains("1 interceptor(s)"));
    assert!(debug.contains("ClientConfig"));
}
