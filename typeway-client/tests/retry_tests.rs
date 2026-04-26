use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use http::StatusCode;
use serde::{Deserialize, Serialize};

use typeway_client::{Client, ClientConfig, ClientError, RetryPolicy};
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

/// A server that returns 503 for the first N requests, then 200.
#[tokio::test]
async fn test_retries_on_503() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let port = start_mock_server(move |_req| {
        let count = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
        async move {
            if count < 3 {
                json_response(StatusCode::SERVICE_UNAVAILABLE, r#""unavailable""#)
            } else {
                json_response(StatusCode::OK, r#"{"status":"ok"}"#)
            }
        }
    })
    .await;

    let config = ClientConfig::default()
        .retry_policy(
            RetryPolicy::default()
                .max_retries(5)
                .initial_backoff(Duration::from_millis(10))
                .max_backoff(Duration::from_millis(100)),
        )
        .no_timeout();

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    let result = client.call::<HealthEndpoint>(()).await.unwrap();
    assert_eq!(
        result,
        HealthResponse {
            status: "ok".into()
        }
    );

    // Should have been called 3 times: 2 failures + 1 success.
    assert_eq!(call_count.load(Ordering::SeqCst), 3);
}

/// Non-retryable status codes (400, 404) should NOT be retried.
#[tokio::test]
async fn test_no_retry_on_400() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let port = start_mock_server(move |_req| {
        call_count_clone.fetch_add(1, Ordering::SeqCst);
        async { json_response(StatusCode::BAD_REQUEST, r#""bad request""#) }
    })
    .await;

    let config = ClientConfig::default()
        .retry_policy(RetryPolicy::default().initial_backoff(Duration::from_millis(10)))
        .no_timeout();

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    let err = client.call::<HealthEndpoint>(()).await.unwrap_err();

    match err {
        ClientError::Status { status, .. } => assert_eq!(status, StatusCode::BAD_REQUEST),
        other => panic!("expected Status error, got: {other:?}"),
    }

    // Only one attempt — no retries.
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

/// Non-retryable 404 should not be retried.
#[tokio::test]
async fn test_no_retry_on_404() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let port = start_mock_server(move |_req| {
        call_count_clone.fetch_add(1, Ordering::SeqCst);
        async { json_response(StatusCode::NOT_FOUND, r#""not found""#) }
    })
    .await;

    let config = ClientConfig::default()
        .retry_policy(RetryPolicy::default().initial_backoff(Duration::from_millis(10)))
        .no_timeout();

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    let err = client.call::<HealthEndpoint>(()).await.unwrap_err();

    match err {
        ClientError::Status { status, .. } => assert_eq!(status, StatusCode::NOT_FOUND),
        other => panic!("expected Status error, got: {other:?}"),
    }

    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

/// `RetryPolicy::none()` should disable all retries.
#[tokio::test]
async fn test_retry_policy_none() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let port = start_mock_server(move |_req| {
        call_count_clone.fetch_add(1, Ordering::SeqCst);
        async { json_response(StatusCode::SERVICE_UNAVAILABLE, r#""unavailable""#) }
    })
    .await;

    let config = ClientConfig::default()
        .retry_policy(RetryPolicy::none())
        .no_timeout();

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    let err = client.call::<HealthEndpoint>(()).await.unwrap_err();

    match err {
        ClientError::Status { status, .. } => assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE),
        other => panic!("expected Status error, got: {other:?}"),
    }

    // Only one attempt — retries disabled.
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

/// When all retries are exhausted, `RetryExhausted` is returned.
#[tokio::test]
async fn test_retry_exhausted() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let port = start_mock_server(move |_req| {
        call_count_clone.fetch_add(1, Ordering::SeqCst);
        async { json_response(StatusCode::SERVICE_UNAVAILABLE, r#""unavailable""#) }
    })
    .await;

    let config = ClientConfig::default()
        .retry_policy(
            RetryPolicy::default()
                .max_retries(2)
                .initial_backoff(Duration::from_millis(10))
                .max_backoff(Duration::from_millis(50)),
        )
        .no_timeout();

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    let err = client.call::<HealthEndpoint>(()).await.unwrap_err();

    match err {
        ClientError::RetryExhausted {
            attempts,
            last_error,
        } => {
            assert_eq!(attempts, 3); // 1 initial + 2 retries
            match *last_error {
                ClientError::Status { status, .. } => {
                    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE)
                }
                other => panic!("expected Status in last_error, got: {other:?}"),
            }
        }
        other => panic!("expected RetryExhausted, got: {other:?}"),
    }

    // 1 initial + 2 retries = 3 total.
    assert_eq!(call_count.load(Ordering::SeqCst), 3);
}

/// Exponential backoff: each retry should take longer than the previous.
#[tokio::test]
async fn test_exponential_backoff_timing() {
    let timestamps = Arc::new(std::sync::Mutex::new(Vec::<Instant>::new()));
    let timestamps_clone = timestamps.clone();

    let port = start_mock_server(move |_req| {
        let ts = timestamps_clone.clone();
        async move {
            ts.lock().unwrap().push(Instant::now());
            json_response(StatusCode::SERVICE_UNAVAILABLE, r#""unavailable""#)
        }
    })
    .await;

    let config = ClientConfig::default()
        .retry_policy(
            RetryPolicy::default()
                .max_retries(3)
                .initial_backoff(Duration::from_millis(50))
                .max_backoff(Duration::from_secs(5))
                .backoff_multiplier(2.0),
        )
        .no_timeout();

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    let _ = client.call::<HealthEndpoint>(()).await;

    let ts = timestamps.lock().unwrap();
    assert_eq!(ts.len(), 4, "expected 4 attempts (1 initial + 3 retries)");

    // Compute the gaps between consecutive attempts.
    let gap1 = ts[1].duration_since(ts[0]);
    let gap2 = ts[2].duration_since(ts[1]);
    let gap3 = ts[3].duration_since(ts[2]);

    // Each gap should be roughly double the previous (with some tolerance
    // for jitter and scheduling). The base values are 50ms, 100ms, 200ms.
    // We just check that each gap is longer than the previous.
    assert!(gap2 > gap1, "gap2 ({gap2:?}) should be > gap1 ({gap1:?})");
    assert!(gap3 > gap2, "gap3 ({gap3:?}) should be > gap2 ({gap2:?})");

    // Sanity: gap1 should be at least ~40ms (the 50ms base minus scheduling).
    assert!(
        gap1 >= Duration::from_millis(30),
        "gap1 ({gap1:?}) should be >= 30ms"
    );
}

/// Retry on 429 Too Many Requests.
#[tokio::test]
async fn test_retry_on_429() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let port = start_mock_server(move |_req| {
        let count = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
        async move {
            if count < 2 {
                json_response(StatusCode::TOO_MANY_REQUESTS, r#""rate limited""#)
            } else {
                json_response(StatusCode::OK, r#"{"status":"ok"}"#)
            }
        }
    })
    .await;

    let config = ClientConfig::default()
        .retry_policy(
            RetryPolicy::default()
                .initial_backoff(Duration::from_millis(10))
                .max_backoff(Duration::from_millis(50)),
        )
        .no_timeout();

    let client = Client::with_config(&format!("http://127.0.0.1:{port}"), config).unwrap();
    let result = client.call::<HealthEndpoint>(()).await.unwrap();
    assert_eq!(
        result,
        HealthResponse {
            status: "ok".into()
        }
    );
    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}
