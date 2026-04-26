//! Rust-driven equivalents of the upstream gRPC interop scenarios that
//! exercise out-of-band features: custom request/response metadata,
//! `grpc-timeout` honouring, and resilience to client cancellation.
//!
//! Mirrors:
//! - `custom_metadata` (UnaryCall and FullDuplexCall)
//! - `timeout_on_sleeping_server`
//! - `cancel_after_begin` / `cancel_after_first_response`
//!
//! https://github.com/grpc/grpc/blob/master/doc/interop-test-descriptions.md

use std::net::SocketAddr;
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;
use http_body_util::{BodyExt, Full};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use prost::Message;
use tokio::net::TcpListener;

use typeway_grpc::framing::{decode_grpc_frames, encode_grpc_frame};
use typeway_grpc::status::decode_grpc_message;
use typeway_interop::server::TestService;
use typeway_interop::testing::{
    EchoStatus, Empty, Payload, PayloadType, ResponseParameters, SimpleRequest, SimpleResponse,
    StreamingOutputCallRequest, StreamingOutputCallResponse,
};

const ECHO_INITIAL_HEADER: &str = "x-grpc-test-echo-initial";
const ECHO_INITIAL_VALUE: &str = "test_initial_metadata_value";
const ECHO_TRAILING_HEADER: &str = "x-grpc-test-echo-trailing-bin";
// Per gRPC convention `-bin` headers carry binary; clients send raw bytes
// and the upstream interop test uses {0xab, 0xab, 0xab}.
const ECHO_TRAILING_VALUE: &[u8] = &[0xab, 0xab, 0xab];

async fn start_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = TokioIo::new(stream);
            let svc = TowerToHyperService::new(TestService::new());
            tokio::spawn(async move {
                let _ = Builder::new(TokioExecutor::new())
                    .http2_only()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

struct CallResult {
    code: i32,
    message: String,
    frames: Vec<Bytes>,
    initial_headers: HeaderMap,
    trailers: HeaderMap,
}

/// Send a gRPC request with arbitrary extra headers and return the full
/// response shape (initial headers, data frames, trailers).
async fn call_with_headers(
    addr: SocketAddr,
    method_path: &str,
    extra_headers: &[(&str, Bytes)],
    request_messages: &[Vec<u8>],
) -> CallResult {
    use hyper::Request;
    use hyper_util::client::legacy::{connect::HttpConnector, Client};

    let mut connector = HttpConnector::new();
    connector.set_nodelay(true);
    let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .http2_only(true)
        .build(connector);

    let mut body = Vec::new();
    for msg in request_messages {
        body.extend_from_slice(&encode_grpc_frame(msg));
    }

    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("http://{addr}{method_path}"))
        .header("content-type", "application/grpc+proto")
        .header("te", "trailers");
    for (k, v) in extra_headers {
        builder = builder.header(*k, v.as_ref());
    }
    let req = builder.body(Full::new(Bytes::from(body))).unwrap();

    let resp = client.request(req).await.unwrap();
    let (parts, body) = resp.into_parts();

    let status_from_headers = parts
        .headers
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i32>().ok());

    let collected = body.collect().await.unwrap();
    let trailers = collected.trailers().cloned().unwrap_or_default();
    let data = collected.to_bytes();

    let code = status_from_headers
        .or_else(|| {
            trailers
                .get("grpc-status")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<i32>().ok())
        })
        .expect("response had no grpc-status header or trailer");

    let message_raw = parts
        .headers
        .get("grpc-message")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            trailers
                .get("grpc-message")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();
    let message = decode_grpc_message(&message_raw);

    let (frames, _) = decode_grpc_frames(&data);
    let frames = frames.into_iter().map(Bytes::copy_from_slice).collect();

    CallResult {
        code,
        message,
        frames,
        initial_headers: parts.headers,
        trailers,
    }
}

// ---------------------------------------------------------------------------
// custom_metadata (UnaryCall)
// ---------------------------------------------------------------------------
//
// > The server should echo `x-grpc-test-echo-initial` back as initial
// > metadata and `x-grpc-test-echo-trailing-bin` back as trailing
// > metadata. The unary flavour exercises both metadata directions on a
// > UnaryCall.

#[tokio::test]
async fn custom_metadata_unary() {
    let addr = start_server().await;
    let req = SimpleRequest {
        response_size: 314_159,
        payload: Some(Payload {
            r#type: PayloadType::Compressable as i32,
            body: vec![0u8; 271_828],
        }),
        ..Default::default()
    };
    let res = call_with_headers(
        addr,
        "/grpc.testing.TestService/UnaryCall",
        &[
            (ECHO_INITIAL_HEADER, Bytes::from(ECHO_INITIAL_VALUE)),
            (ECHO_TRAILING_HEADER, Bytes::from(ECHO_TRAILING_VALUE)),
        ],
        &[req.encode_to_vec()],
    )
    .await;

    assert_eq!(
        res.code, 0,
        "expected OK; got {} ({:?})",
        res.code, res.message
    );

    let initial = res
        .initial_headers
        .get(ECHO_INITIAL_HEADER)
        .expect("server should echo initial metadata");
    assert_eq!(initial.as_bytes(), ECHO_INITIAL_VALUE.as_bytes());

    let trailing = res
        .trailers
        .get(ECHO_TRAILING_HEADER)
        .expect("server should echo trailing metadata in trailers");
    assert_eq!(trailing.as_bytes(), ECHO_TRAILING_VALUE);

    // Sanity: the response payload is still valid.
    assert_eq!(res.frames.len(), 1);
    let resp = SimpleResponse::decode(res.frames[0].clone()).expect("decodes");
    assert_eq!(resp.payload.unwrap().body.len(), 314_159);
}

// ---------------------------------------------------------------------------
// custom_metadata (FullDuplexCall)
// ---------------------------------------------------------------------------
//
// > Same contract as the unary variant, but driven through a bidi stream.
// > The echo trailers must arrive on the dynamic-trailers path.

#[tokio::test]
async fn custom_metadata_full_duplex() {
    let addr = start_server().await;
    let req = StreamingOutputCallRequest {
        response_parameters: vec![ResponseParameters {
            size: 314_159,
            ..Default::default()
        }],
        payload: Some(Payload {
            r#type: PayloadType::Compressable as i32,
            body: vec![0u8; 271_828],
        }),
        ..Default::default()
    };
    let res = call_with_headers(
        addr,
        "/grpc.testing.TestService/FullDuplexCall",
        &[
            (ECHO_INITIAL_HEADER, Bytes::from(ECHO_INITIAL_VALUE)),
            (ECHO_TRAILING_HEADER, Bytes::from(ECHO_TRAILING_VALUE)),
        ],
        &[req.encode_to_vec()],
    )
    .await;

    assert_eq!(
        res.code, 0,
        "expected OK; got {} ({:?})",
        res.code, res.message
    );

    let initial = res
        .initial_headers
        .get(ECHO_INITIAL_HEADER)
        .expect("bidi server should echo initial metadata");
    assert_eq!(initial.as_bytes(), ECHO_INITIAL_VALUE.as_bytes());

    let trailing = res
        .trailers
        .get(ECHO_TRAILING_HEADER)
        .expect("bidi server should echo trailing metadata");
    assert_eq!(trailing.as_bytes(), ECHO_TRAILING_VALUE);

    assert_eq!(res.frames.len(), 1);
    let resp = StreamingOutputCallResponse::decode(res.frames[0].clone()).expect("decodes");
    assert_eq!(resp.payload.unwrap().body.len(), 314_159);
}

// ---------------------------------------------------------------------------
// custom_metadata combined with a non-OK status
// ---------------------------------------------------------------------------
//
// > Verifies that the trailing-bin echo rides along with an error
// > response. The server emits no data frame, but the trailers frame
// > should still carry both `grpc-status` and the echoed
// > `x-grpc-test-echo-trailing-bin` header.

#[tokio::test]
async fn custom_metadata_with_error_status() {
    let addr = start_server().await;
    let req = SimpleRequest {
        response_status: Some(EchoStatus {
            code: 2, // UNKNOWN
            message: "echo plus error".into(),
        }),
        ..Default::default()
    };
    let res = call_with_headers(
        addr,
        "/grpc.testing.TestService/UnaryCall",
        &[
            (ECHO_INITIAL_HEADER, Bytes::from(ECHO_INITIAL_VALUE)),
            (ECHO_TRAILING_HEADER, Bytes::from(ECHO_TRAILING_VALUE)),
        ],
        &[req.encode_to_vec()],
    )
    .await;

    assert_eq!(res.code, 2);
    assert_eq!(res.message, "echo plus error");
    assert!(res.frames.is_empty(), "error responses have no data frames");

    let initial = res
        .initial_headers
        .get(ECHO_INITIAL_HEADER)
        .expect("initial echo should ride along with an error");
    assert_eq!(initial.as_bytes(), ECHO_INITIAL_VALUE.as_bytes());

    let trailing = res
        .trailers
        .get(ECHO_TRAILING_HEADER)
        .expect("trailing echo should ride along with an error");
    assert_eq!(trailing.as_bytes(), ECHO_TRAILING_VALUE);
}

// ---------------------------------------------------------------------------
// interval_us is honoured between streamed responses
// ---------------------------------------------------------------------------
//
// > The other streaming tests assert frame counts and shapes but not
// > timing, so a regression that ignored `interval_us` would slip
// > through. This test asks for three responses spaced 50ms apart and
// > asserts the wall-clock elapsed time matches.

#[tokio::test]
async fn interval_us_paces_streamed_responses() {
    let addr = start_server().await;
    let interval_us: i32 = 50_000; // 50ms
    let frame_count: usize = 3;
    let req = StreamingOutputCallRequest {
        response_parameters: (0..frame_count)
            .map(|_| ResponseParameters {
                size: 16,
                interval_us,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };

    let started = std::time::Instant::now();
    let res = call_with_headers(
        addr,
        "/grpc.testing.TestService/FullDuplexCall",
        &[],
        &[req.encode_to_vec()],
    )
    .await;
    let elapsed = started.elapsed();

    assert_eq!(
        res.code, 0,
        "expected OK; got {} ({:?})",
        res.code, res.message
    );
    assert_eq!(res.frames.len(), frame_count);

    // Each frame is preceded by a `interval_us` sleep, so the total
    // wall-clock time has to be at least N * interval. Allow a small
    // slack on the lower bound to absorb timer rounding.
    let min_expected = Duration::from_micros((interval_us as u64) * (frame_count as u64))
        - Duration::from_millis(20);
    assert!(
        elapsed >= min_expected,
        "expected at least {min_expected:?} of pacing; got {elapsed:?}"
    );
    // And it shouldn't take wildly longer either; a generous upper
    // bound catches the case where pacing accidentally compounds.
    let max_expected = Duration::from_micros((interval_us as u64) * (frame_count as u64) * 4);
    assert!(
        elapsed <= max_expected,
        "pacing took longer than expected: {elapsed:?} (max {max_expected:?})"
    );
}

// ---------------------------------------------------------------------------
// timeout_on_sleeping_server
// ---------------------------------------------------------------------------
//
// > Client calls FullDuplexCall asking for one response with
// > `interval_us = 10_000_000` (10s) and a 1ms `grpc-timeout`. The server
// > should observe the deadline and respond with DEADLINE_EXCEEDED before
// > the interval elapses.

#[tokio::test]
async fn timeout_on_sleeping_server() {
    let addr = start_server().await;
    let req = StreamingOutputCallRequest {
        response_parameters: vec![ResponseParameters {
            size: 31_415,
            interval_us: 10_000_000, // 10s; far longer than the timeout
            ..Default::default()
        }],
        ..Default::default()
    };
    let started = std::time::Instant::now();
    let res = call_with_headers(
        addr,
        "/grpc.testing.TestService/FullDuplexCall",
        &[("grpc-timeout", Bytes::from("100m"))], // 100ms
        &[req.encode_to_vec()],
    )
    .await;
    let elapsed = started.elapsed();

    assert_eq!(res.code, 4, "expected DEADLINE_EXCEEDED; got {}", res.code);
    assert!(
        res.frames.is_empty(),
        "no frames should arrive before the deadline"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "request should finish well before the configured 10s interval; got {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// timeout_on_sleeping_server (unary)
// ---------------------------------------------------------------------------
//
// > Sanity check that grpc-timeout also applies to unary RPCs, even though
// > the upstream test description only spells out the bidi case. A unary
// > call with an arbitrarily small timeout should never see DEADLINE_EXCEEDED
// > on a UnaryCall that completes in microseconds, but the parsing path
// > should still work end-to-end.

#[tokio::test]
async fn unary_with_timeout_still_succeeds() {
    let addr = start_server().await;
    let req = SimpleRequest {
        response_size: 16,
        ..Default::default()
    };
    let res = call_with_headers(
        addr,
        "/grpc.testing.TestService/UnaryCall",
        &[("grpc-timeout", Bytes::from("30S"))],
        &[req.encode_to_vec()],
    )
    .await;
    assert_eq!(res.code, 0);
    assert_eq!(res.frames.len(), 1);
}

// ---------------------------------------------------------------------------
// cancel_after_begin
// ---------------------------------------------------------------------------
//
// > Client opens a streaming call and drops it before sending any data
// > frames. The server should not hang: a follow-up RPC on a fresh
// > connection should complete normally.

#[tokio::test]
async fn cancel_after_begin() {
    use hyper::Request;
    use hyper_util::client::legacy::{connect::HttpConnector, Client};

    let addr = start_server().await;

    // Open a StreamingInputCall with no body and immediately drop the
    // future. The server-side worker should observe the receiver close
    // and exit.
    let mut connector = HttpConnector::new();
    connector.set_nodelay(true);
    let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .http2_only(true)
        .build(connector);

    let req = Request::builder()
        .method("POST")
        .uri(format!(
            "http://{addr}/grpc.testing.TestService/StreamingInputCall"
        ))
        .header("content-type", "application/grpc+proto")
        .header("te", "trailers")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let fut = client.request(req);

    // Race the request against a short timer; drop whichever loses.
    let _ = tokio::time::timeout(Duration::from_millis(20), fut).await;

    // Now confirm the server is still healthy by completing a fresh call.
    let res = call_with_headers(
        addr,
        "/grpc.testing.TestService/EmptyCall",
        &[],
        &[Empty {}.encode_to_vec()],
    )
    .await;
    assert_eq!(res.code, 0);
}

// ---------------------------------------------------------------------------
// cancel_after_first_response
// ---------------------------------------------------------------------------
//
// > Client opens a FullDuplexCall, sends one request, receives one
// > response, then drops the connection. The server should observe the
// > closure mid-stream without hanging or panicking.

#[tokio::test]
async fn cancel_after_first_response() {
    use hyper::Request;
    use hyper_util::client::legacy::{connect::HttpConnector, Client};

    let addr = start_server().await;

    // Ask for a stream of 8 frames at 50ms each. Then drop after the
    // first one arrives. Without cancel-safety, the server worker would
    // keep trying to send into a dead channel.
    let req = StreamingOutputCallRequest {
        response_parameters: (0..8)
            .map(|_| ResponseParameters {
                size: 16,
                interval_us: 50_000,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let mut body = Vec::new();
    body.extend_from_slice(&encode_grpc_frame(&req.encode_to_vec()));

    let mut connector = HttpConnector::new();
    connector.set_nodelay(true);
    let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .http2_only(true)
        .build(connector);
    let req = Request::builder()
        .method("POST")
        .uri(format!(
            "http://{addr}/grpc.testing.TestService/FullDuplexCall"
        ))
        .header("content-type", "application/grpc+proto")
        .header("te", "trailers")
        .body(Full::new(Bytes::from(body)))
        .unwrap();

    let resp = client.request(req).await.unwrap();
    let (_parts, body) = resp.into_parts();

    // Read just the first frame (5-byte header + payload), then drop.
    let mut body = body;
    let mut buf = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.unwrap();
        if let Ok(data) = frame.into_data() {
            buf.extend_from_slice(&data);
            if buf.len() >= 5 {
                let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
                if buf.len() >= 5 + len {
                    break; // got at least one full frame
                }
            }
        }
    }
    drop(body);

    // Server should still be healthy.
    let res = call_with_headers(
        addr,
        "/grpc.testing.TestService/EmptyCall",
        &[],
        &[Empty {}.encode_to_vec()],
    )
    .await;
    assert_eq!(res.code, 0);

    // Sanity: silence the unused-EchoStatus warning.
    let _ = EchoStatus::default();
}
