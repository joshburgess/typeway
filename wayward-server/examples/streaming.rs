//! Demonstrates streaming responses and Server-Sent Events.
//!
//! Run: cargo run -p wayward-server --example streaming
//! Test:
//!   curl http://127.0.0.1:3000/stream   # chunked response
//!   curl http://127.0.0.1:3000/sse      # SSE event stream

use std::time::Duration;

use wayward_core::*;
use wayward_macros::*;
use wayward_server::body::{body_from_stream, sse_body, BoxBody};
use wayward_server::*;

wayward_path!(type StreamPath = "stream");
wayward_path!(type SsePath = "sse");

type API = (
    GetEndpoint<StreamPath, String>,
    GetEndpoint<SsePath, String>,
);

/// Handler that returns a chunked streaming response.
async fn stream() -> http::Response<BoxBody> {
    let stream = futures::stream::iter(vec![
        Ok(http_body::Frame::data(bytes::Bytes::from("chunk 1\n"))),
        Ok(http_body::Frame::data(bytes::Bytes::from("chunk 2\n"))),
        Ok(http_body::Frame::data(bytes::Bytes::from("chunk 3\n"))),
    ]);

    let mut res = http::Response::new(body_from_stream(stream));
    res.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("text/plain"),
    );
    res
}

/// Handler that returns a Server-Sent Events stream.
async fn sse() -> http::Response<BoxBody> {
    use futures::StreamExt;

    // Create a stream that emits events every second.
    let events = futures::stream::unfold(0u32, |count| async move {
        if count >= 5 {
            return None;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        Some((format!("event {count}"), count + 1))
    });

    let mut res = http::Response::new(sse_body(events));
    res.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("text/event-stream"),
    );
    res.headers_mut().insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-cache"),
    );
    res
}

#[tokio::main]
async fn main() {
    let server = Server::<API>::new((bind::<_, _, _>(stream), bind::<_, _, _>(sse)));

    println!("Streaming example on http://127.0.0.1:3000");
    println!("  GET /stream - chunked text response");
    println!("  GET /sse    - Server-Sent Events (5 events, 1/sec)");

    server
        .serve("127.0.0.1:3000".parse().unwrap())
        .await
        .unwrap();
}
