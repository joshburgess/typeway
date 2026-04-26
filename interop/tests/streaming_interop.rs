//! Rust-driven equivalents of the upstream gRPC interop streaming
//! scenarios.
//!
//! These cover the server-streaming, client-streaming, and bidi streaming
//! tests from the upstream interop test descriptions:
//! https://github.com/grpc/grpc/blob/master/doc/interop-test-descriptions.md
//!
//! For ping_pong the wire-level assertion is "for each request frame the
//! server emits a response frame of the requested size, in order." The
//! tests submit all request frames in a single HTTP/2 body chunk; the
//! server's [`GrpcFrameReader`] still reads them one at a time and the
//! response stream still arrives as a sequence of independent frames, so
//! the assertion is identical to what the upstream `interop_client`
//! verifies via turn-taking.

use std::net::SocketAddr;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use prost::Message;
use tokio::net::TcpListener;

use typeway_grpc::framing::{decode_grpc_frames, encode_grpc_frame};
use typeway_interop::server::TestService;
use typeway_interop::testing::{
    Payload, PayloadType, ResponseParameters, StreamingInputCallRequest,
    StreamingInputCallResponse, StreamingOutputCallRequest, StreamingOutputCallResponse,
};

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

/// Send a streaming gRPC request (one or more framed messages
/// concatenated in the body) and return (`grpc-status`, decoded response
/// frames, `grpc-message`).
async fn streaming_call(
    addr: SocketAddr,
    method_path: &str,
    request_messages: &[Vec<u8>],
) -> (i32, Vec<Bytes>, String) {
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

    let req = Request::builder()
        .method("POST")
        .uri(format!("http://{addr}{method_path}"))
        .header("content-type", "application/grpc+proto")
        .header("te", "trailers")
        .body(Full::new(Bytes::from(body)))
        .unwrap();

    let resp = client.request(req).await.unwrap();
    let (parts, body) = resp.into_parts();

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

    let message = trailers
        .as_ref()
        .and_then(|t| t.get("grpc-message"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let (frames, _trailers_frame) = decode_grpc_frames(&data);
    let frames = frames.into_iter().map(Bytes::copy_from_slice).collect();

    (status_code, frames, message)
}

// ---------------------------------------------------------------------------
// server_streaming
// ---------------------------------------------------------------------------
//
// > Single request, multiple responses. The client requests a list of
// > response sizes, and the server emits one StreamingOutputCallResponse
// > per requested size with a payload of that size.

const SERVER_STREAM_SIZES: &[i32] = &[31_415, 9, 2_653, 58_979];

#[tokio::test]
async fn server_streaming() {
    let addr = start_server().await;
    let req = StreamingOutputCallRequest {
        response_type: PayloadType::Compressable as i32,
        response_parameters: SERVER_STREAM_SIZES
            .iter()
            .map(|&size| ResponseParameters {
                size,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };

    let (code, frames, msg) = streaming_call(
        addr,
        "/grpc.testing.TestService/StreamingOutputCall",
        &[req.encode_to_vec()],
    )
    .await;

    assert_eq!(code, 0, "expected OK; got grpc-status {code} ({msg:?})");
    assert_eq!(frames.len(), SERVER_STREAM_SIZES.len());
    for (frame, expected_size) in frames.iter().zip(SERVER_STREAM_SIZES.iter()) {
        let resp = StreamingOutputCallResponse::decode(frame.clone()).expect("decodes");
        let payload = resp.payload.expect("payload present");
        assert_eq!(payload.body.len(), *expected_size as usize);
    }
}

// ---------------------------------------------------------------------------
// client_streaming
// ---------------------------------------------------------------------------
//
// > Multiple requests, single response. The client sends a sequence of
// > StreamingInputCallRequest messages each carrying a Payload, and the
// > server replies with one StreamingInputCallResponse whose
// > aggregated_payload_size equals the sum of the input payload sizes.

const CLIENT_STREAM_SIZES: &[usize] = &[27_182, 8, 1_828, 45_904];

#[tokio::test]
async fn client_streaming() {
    let addr = start_server().await;
    let messages: Vec<Vec<u8>> = CLIENT_STREAM_SIZES
        .iter()
        .map(|&size| {
            StreamingInputCallRequest {
                payload: Some(Payload {
                    r#type: PayloadType::Compressable as i32,
                    body: vec![0u8; size],
                }),
                ..Default::default()
            }
            .encode_to_vec()
        })
        .collect();

    let (code, frames, msg) = streaming_call(
        addr,
        "/grpc.testing.TestService/StreamingInputCall",
        &messages,
    )
    .await;

    assert_eq!(code, 0, "expected OK; got grpc-status {code} ({msg:?})");
    assert_eq!(frames.len(), 1);
    let resp = StreamingInputCallResponse::decode(frames[0].clone()).expect("decodes");
    let expected: i32 = CLIENT_STREAM_SIZES.iter().map(|&n| n as i32).sum();
    assert_eq!(resp.aggregated_payload_size, expected);
}

// ---------------------------------------------------------------------------
// ping_pong (FullDuplexCall)
// ---------------------------------------------------------------------------
//
// > Bidi streaming. For each request frame carrying response_parameters,
// > the server emits a response frame with a payload of the requested
// > size, before reading the next request frame.

const PING_PONG_SIZES: &[i32] = &[31_415, 9, 2_653, 58_979];

#[tokio::test]
async fn ping_pong() {
    let addr = start_server().await;
    let messages: Vec<Vec<u8>> = PING_PONG_SIZES
        .iter()
        .map(|&size| {
            StreamingOutputCallRequest {
                response_type: PayloadType::Compressable as i32,
                response_parameters: vec![ResponseParameters {
                    size,
                    ..Default::default()
                }],
                payload: Some(Payload {
                    r#type: PayloadType::Compressable as i32,
                    body: vec![0u8; size as usize],
                }),
                ..Default::default()
            }
            .encode_to_vec()
        })
        .collect();

    let (code, frames, msg) = streaming_call(
        addr,
        "/grpc.testing.TestService/FullDuplexCall",
        &messages,
    )
    .await;

    assert_eq!(code, 0, "expected OK; got grpc-status {code} ({msg:?})");
    assert_eq!(frames.len(), PING_PONG_SIZES.len());
    for (frame, expected_size) in frames.iter().zip(PING_PONG_SIZES.iter()) {
        let resp = StreamingOutputCallResponse::decode(frame.clone()).expect("decodes");
        let payload = resp.payload.expect("payload present");
        assert_eq!(payload.body.len(), *expected_size as usize);
    }
}

// ---------------------------------------------------------------------------
// empty_stream
// ---------------------------------------------------------------------------
//
// > Bidi streaming with no input and no output. The client closes its
// > send half immediately; the server should respond with no frames and
// > a trailers-only OK status.

#[tokio::test]
async fn empty_stream() {
    let addr = start_server().await;
    let (code, frames, msg) =
        streaming_call(addr, "/grpc.testing.TestService/FullDuplexCall", &[]).await;

    assert_eq!(code, 0, "expected OK; got grpc-status {code} ({msg:?})");
    assert!(frames.is_empty(), "empty_stream should yield no frames");
}

// ---------------------------------------------------------------------------
// half_duplex (smoke)
// ---------------------------------------------------------------------------
//
// > HalfDuplexCall is structurally identical to FullDuplexCall on the
// > wire (the upstream description differs only in client behaviour).
// > Smoke-test that the method exists and returns a frame per request.

#[tokio::test]
async fn half_duplex_smoke() {
    let addr = start_server().await;
    let req = StreamingOutputCallRequest {
        response_parameters: vec![ResponseParameters {
            size: 16,
            ..Default::default()
        }],
        ..Default::default()
    };
    let (code, frames, _) = streaming_call(
        addr,
        "/grpc.testing.TestService/HalfDuplexCall",
        &[req.encode_to_vec()],
    )
    .await;

    assert_eq!(code, 0);
    assert_eq!(frames.len(), 1);
}
