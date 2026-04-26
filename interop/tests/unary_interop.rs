//! Rust-driven equivalents of the upstream gRPC interop test scenarios.
//!
//! Each test mirrors a named scenario from
//! https://github.com/grpc/grpc/blob/master/doc/interop-test-descriptions.md
//! and asserts the wire-level outcome the official client would assert.
//!
//! These tests exercise the same code paths a real `grpc-go` interop_client
//! would, with the request/response messages encoded by `prost` exactly as
//! upstream consumers do. Streaming scenarios live in a follow-up commit
//! once typeway-grpc grows a streaming `DirectHandler` story.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use prost::Message;
use tokio::net::TcpListener;

use typeway_interop::server::TestService;
use typeway_interop::testing::{
    EchoStatus, Empty, Payload, PayloadType, SimpleRequest, SimpleResponse,
};

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

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
    // Brief pause to let the listener settle before the first request.
    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

/// Send a unary gRPC request and return (`grpc-status`, body bytes,
/// `grpc-message`).
async fn unary_call(
    addr: SocketAddr,
    method_path: &str,
    request_bytes: Vec<u8>,
) -> (i32, Bytes, String) {
    use hyper::Request;
    use hyper_util::client::legacy::{connect::HttpConnector, Client};

    let mut connector = HttpConnector::new();
    connector.set_nodelay(true);
    let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .http2_only(true)
        .build(connector);

    // gRPC frame: 1-byte compressed flag + 4-byte length + payload.
    let mut framed = Vec::with_capacity(5 + request_bytes.len());
    framed.push(0);
    framed.extend_from_slice(&(request_bytes.len() as u32).to_be_bytes());
    framed.extend_from_slice(&request_bytes);

    let req = Request::builder()
        .method("POST")
        .uri(format!("http://{addr}{method_path}"))
        .header("content-type", "application/grpc+proto")
        .header("te", "trailers")
        .body(Full::new(Bytes::from(framed)))
        .unwrap();

    let resp = client.request(req).await.unwrap();
    let (parts, body) = resp.into_parts();

    // grpc-status / grpc-message can come back as headers (for trailers-only
    // responses) or inside the trailers frame.
    let status_from_headers = parts
        .headers
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i32>().ok());

    let collected = body.collect().await.unwrap();
    let trailers = collected.trailers().cloned();
    let data = collected.to_bytes();

    let status_code = status_from_headers
        .or_else(|| {
            trailers
                .as_ref()
                .and_then(|t| t.get("grpc-status"))
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
                .as_ref()
                .and_then(|t| t.get("grpc-message"))
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();
    let message = percent_decode_grpc_message(&message_raw);

    // Strip the gRPC frame from the data, if there is one.
    let payload = if data.len() >= 5 {
        let len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
        if data.len() >= 5 + len {
            data.slice(5..5 + len)
        } else {
            data.slice(5..)
        }
    } else {
        Bytes::new()
    };

    (status_code, payload, message)
}

// ---------------------------------------------------------------------------
// empty_unary
// ---------------------------------------------------------------------------
//
// > This test verifies that gRPC requests are passed and responses returned
// > with no payload.

#[tokio::test]
async fn empty_unary() {
    let addr = start_server().await;
    let (code, body, msg) = unary_call(
        addr,
        "/grpc.testing.TestService/EmptyCall",
        Empty {}.encode_to_vec(),
    )
    .await;
    assert_eq!(code, 0, "expected OK; got grpc-status {code} ({msg:?})");
    let decoded = Empty::decode(body).expect("response decodes as Empty");
    assert_eq!(decoded, Empty {});
}

// ---------------------------------------------------------------------------
// large_unary
// ---------------------------------------------------------------------------
//
// > This test verifies unary calls succeed in sending messages, and `payload`
// > is filled with random data, then echoed back. Body sizes are 271828
// > and 314159, the magic numbers from the upstream tests.

const LARGE_REQUEST_PAYLOAD_SIZE: usize = 271_828;
const LARGE_RESPONSE_PAYLOAD_SIZE: i32 = 314_159;

#[tokio::test]
async fn large_unary() {
    let addr = start_server().await;

    let req = SimpleRequest {
        response_type: PayloadType::Compressable as i32,
        response_size: LARGE_RESPONSE_PAYLOAD_SIZE,
        payload: Some(Payload {
            r#type: PayloadType::Compressable as i32,
            body: vec![0u8; LARGE_REQUEST_PAYLOAD_SIZE],
        }),
        ..Default::default()
    };
    let (code, body, msg) = unary_call(
        addr,
        "/grpc.testing.TestService/UnaryCall",
        req.encode_to_vec(),
    )
    .await;

    assert_eq!(code, 0, "expected OK; got grpc-status {code} ({msg:?})");
    let resp = SimpleResponse::decode(body).expect("response decodes as SimpleResponse");
    let payload = resp.payload.expect("response has payload");
    assert_eq!(payload.body.len(), LARGE_RESPONSE_PAYLOAD_SIZE as usize);
}

// ---------------------------------------------------------------------------
// status_code_and_message
// ---------------------------------------------------------------------------
//
// > Verifies that a request can specify the `EchoStatus` it wants back,
// > and that both the code and the message reach the client.

#[tokio::test]
async fn status_code_and_message() {
    let addr = start_server().await;

    let req = SimpleRequest {
        response_status: Some(EchoStatus {
            code: 2, // UNKNOWN
            message: "test status message".into(),
        }),
        ..Default::default()
    };
    let (code, body, msg) = unary_call(
        addr,
        "/grpc.testing.TestService/UnaryCall",
        req.encode_to_vec(),
    )
    .await;

    assert_eq!(code, 2, "expected UNKNOWN; got grpc-status {code}");
    assert_eq!(msg, "test status message");
    assert!(body.is_empty(), "error responses have no body");
}

// ---------------------------------------------------------------------------
// special_status_message
// ---------------------------------------------------------------------------
//
// > Like `status_code_and_message` but the message contains characters that
// > require percent-escaping per the grpc-message spec (CR, LF, and a unicode
// > codepoint). The client should see the unescaped string.

#[tokio::test]
async fn special_status_message() {
    let addr = start_server().await;
    let special = "\t\n\r unicode: 文字"; // tab, LF, CR, and CJK
    let req = SimpleRequest {
        response_status: Some(EchoStatus {
            code: 2,
            message: special.into(),
        }),
        ..Default::default()
    };
    let (code, _, msg) = unary_call(
        addr,
        "/grpc.testing.TestService/UnaryCall",
        req.encode_to_vec(),
    )
    .await;

    assert_eq!(code, 2);
    assert_eq!(msg, special, "unescaped grpc-message should round-trip");
}

// ---------------------------------------------------------------------------
// unimplemented_method
// ---------------------------------------------------------------------------
//
// > Calling `TestService/UnimplementedCall` should yield UNIMPLEMENTED.

#[tokio::test]
async fn unimplemented_method() {
    let addr = start_server().await;
    let (code, _, _) = unary_call(
        addr,
        "/grpc.testing.TestService/UnimplementedCall",
        Empty {}.encode_to_vec(),
    )
    .await;
    assert_eq!(code, 12, "expected UNIMPLEMENTED");
}

// ---------------------------------------------------------------------------
// unimplemented_service
// ---------------------------------------------------------------------------
//
// > Calling any method on `UnimplementedService` should yield UNIMPLEMENTED.

#[tokio::test]
async fn unimplemented_service() {
    let addr = start_server().await;
    let (code, _, _) = unary_call(
        addr,
        "/grpc.testing.UnimplementedService/UnimplementedCall",
        Empty {}.encode_to_vec(),
    )
    .await;
    assert_eq!(code, 12, "expected UNIMPLEMENTED");
}

// ---------------------------------------------------------------------------
// cacheable_unary (smoke)
// ---------------------------------------------------------------------------
//
// > The full cacheable_unary test verifies HTTP cache behaviour through a
// > GFE-style proxy. Without that proxy, all this test asserts is that the
// > method exists and behaves like UnaryCall.

#[tokio::test]
async fn cacheable_unary_smoke() {
    let addr = start_server().await;
    let req = SimpleRequest {
        response_size: 8,
        ..Default::default()
    };
    let (code, body, _) = unary_call(
        addr,
        "/grpc.testing.TestService/CacheableUnaryCall",
        req.encode_to_vec(),
    )
    .await;
    assert_eq!(code, 0);
    let resp = SimpleResponse::decode(body).expect("decodes");
    assert_eq!(resp.payload.unwrap().body.len(), 8);
}

// Make `Infallible` reachable for any future helper that needs it.
const _: Option<Infallible> = None;

/// Percent-decode a `grpc-message` header value.
///
/// The official gRPC interop client decodes percent-escaped sequences in
/// `grpc-message` and presents the unescaped string to user code, so the
/// test harness has to do the same.
fn percent_decode_grpc_message(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}
