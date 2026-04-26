//! Server-Sent Events response support.
//!
//! [`SseResponse`] is an [`IntoResponse`] wrapper for any stream of
//! [`SseEvent`] values. It sets the SSE-mandated headers (`Content-Type:
//! text/event-stream`, `Cache-Control: no-cache`) and frames each event per
//! the WHATWG SSE spec.
//!
//! # Example
//!
//! ```ignore
//! use futures::stream;
//! use typeway_server::{SseEvent, SseResponse};
//!
//! async fn ticker() -> SseResponse<impl futures::Stream<Item = SseEvent>> {
//!     let s = stream::iter((0..3).map(|i| SseEvent::data(format!("tick {i}"))));
//!     SseResponse::new(s)
//! }
//! ```

use std::time::Duration;

use bytes::Bytes;
use futures::Stream;

use crate::body::{body_from_stream, BoxBody, BoxBodyError};
use crate::response::IntoResponse;

/// A single Server-Sent Event.
///
/// Fields are formatted per the WHATWG SSE spec. Multi-line `data` strings
/// are split on `\n` and emitted as separate `data:` lines, which the client
/// concatenates back into a single message.
///
/// Construct comment-only frames (used for keep-alive heartbeats) with
/// [`SseEvent::comment`]. Comments are written as `:<text>\n\n` and ignored
/// by clients.
#[derive(Debug, Clone, Default)]
pub struct SseEvent {
    /// Optional event ID, sent as `id: <value>`.
    pub id: Option<String>,
    /// Optional event name, sent as `event: <value>`.
    pub event: Option<String>,
    /// The event payload, sent as one or more `data: <line>` lines.
    pub data: String,
    /// Optional reconnection time hint in milliseconds, sent as `retry: <ms>`.
    pub retry: Option<u64>,
    /// Comment text. When `Some`, the event is rendered as `:<text>\n\n` and
    /// all other fields are ignored.
    comment: Option<String>,
}

impl SseEvent {
    /// Create a bare data-only event.
    pub fn data(data: impl Into<String>) -> Self {
        SseEvent {
            data: data.into(),
            ..Default::default()
        }
    }

    /// Create a comment-only frame (e.g. `:keepalive`).
    ///
    /// Comments are ignored by SSE clients per the spec but keep the
    /// connection alive through proxies that close idle TCP connections.
    pub fn comment(text: impl Into<String>) -> Self {
        SseEvent {
            comment: Some(text.into()),
            ..Default::default()
        }
    }

    /// Set the event ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the event name.
    pub fn with_event(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());
        self
    }

    /// Set the reconnection time hint in milliseconds.
    pub fn with_retry(mut self, ms: u64) -> Self {
        self.retry = Some(ms);
        self
    }

    /// Format the event as an SSE-spec wire frame.
    ///
    /// Multi-line `data` is split into separate `data:` lines. Carriage
    /// returns and embedded newlines inside scalar fields are stripped so
    /// they cannot terminate the frame prematurely. The frame ends with a
    /// blank line.
    pub fn to_wire(&self) -> String {
        if let Some(ref c) = self.comment {
            let mut out = String::new();
            for line in c.split('\n') {
                out.push(':');
                out.push_str(&line.replace('\r', ""));
                out.push('\n');
            }
            out.push('\n');
            return out;
        }

        let mut out = String::new();
        if let Some(ref id) = self.id {
            out.push_str("id: ");
            out.push_str(&strip_cr_lf(id));
            out.push('\n');
        }
        if let Some(ref ev) = self.event {
            out.push_str("event: ");
            out.push_str(&strip_cr_lf(ev));
            out.push('\n');
        }
        if let Some(ms) = self.retry {
            out.push_str("retry: ");
            out.push_str(&ms.to_string());
            out.push('\n');
        }
        for line in self.data.split('\n') {
            out.push_str("data: ");
            out.push_str(&line.replace('\r', ""));
            out.push('\n');
        }
        out.push('\n');
        out
    }
}

fn strip_cr_lf(s: &str) -> String {
    s.chars().filter(|&c| c != '\n' && c != '\r').collect()
}

/// An SSE response: any stream of [`SseEvent`] values, plus the right headers.
///
/// Returns a `Response<BoxBody>` with `Content-Type: text/event-stream`,
/// `Cache-Control: no-cache`, `X-Accel-Buffering: no`, and `Connection:
/// keep-alive`. The response status is `200 OK`.
pub struct SseResponse<S> {
    stream: S,
}

impl<S> SseResponse<S>
where
    S: Stream<Item = SseEvent> + Send + 'static,
{
    /// Wrap a stream of events.
    pub fn new(stream: S) -> Self {
        SseResponse { stream }
    }
}

impl<S> IntoResponse for SseResponse<S>
where
    S: Stream<Item = SseEvent> + Send + 'static,
{
    fn into_response(self) -> http::Response<BoxBody> {
        use futures::StreamExt;
        let framed = self.stream.map(|ev| {
            let wire = ev.to_wire();
            Ok::<_, BoxBodyError>(http_body::Frame::data(Bytes::from(wire)))
        });
        let body = body_from_stream(framed);
        let mut res = http::Response::new(body);
        let h = res.headers_mut();
        h.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );
        h.insert(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("no-cache"),
        );
        h.insert(
            http::header::CONNECTION,
            http::HeaderValue::from_static("keep-alive"),
        );
        // Disables proxy buffering on nginx so events flush in real time.
        h.insert(
            http::HeaderName::from_static("x-accel-buffering"),
            http::HeaderValue::from_static("no"),
        );
        res
    }
}

/// Inject periodic comment frames (`:keepalive\n\n`) into an SSE stream.
///
/// Many HTTP proxies and load balancers close idle connections after 30-60s.
/// This helper interleaves the original events with synthetic keep-alive
/// comments emitted at `interval`. Comments are ignored by clients per the
/// SSE spec.
pub fn keep_alive<S>(stream: S, interval: Duration) -> impl Stream<Item = SseEvent>
where
    S: Stream<Item = SseEvent> + Send + 'static,
{
    let pings = futures::stream::unfold((), move |_| async move {
        tokio::time::sleep(interval).await;
        Some((SseEvent::comment("keepalive"), ()))
    });
    futures::stream::select(stream, pings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_event_wire_format() {
        let ev = SseEvent::data("hello");
        assert_eq!(ev.to_wire(), "data: hello\n\n");
    }

    #[test]
    fn multi_line_data_splits_into_multiple_data_lines() {
        let ev = SseEvent::data("line one\nline two\nline three");
        assert_eq!(
            ev.to_wire(),
            "data: line one\ndata: line two\ndata: line three\n\n"
        );
    }

    #[test]
    fn full_event_wire_format() {
        let ev = SseEvent::data("payload")
            .with_id("42")
            .with_event("update")
            .with_retry(3000);
        assert_eq!(
            ev.to_wire(),
            "id: 42\nevent: update\nretry: 3000\ndata: payload\n\n"
        );
    }

    #[test]
    fn comment_event_wire_format() {
        let ev = SseEvent::comment("keepalive");
        assert_eq!(ev.to_wire(), ":keepalive\n\n");
    }

    #[test]
    fn comment_with_newline_is_split() {
        let ev = SseEvent::comment("line1\nline2");
        assert_eq!(ev.to_wire(), ":line1\n:line2\n\n");
    }

    #[test]
    fn cr_lf_is_stripped_from_scalar_fields() {
        let ev = SseEvent::data("ok").with_id("1\n2\r3").with_event("a\nb");
        let wire = ev.to_wire();
        assert!(wire.contains("id: 123\n"));
        assert!(wire.contains("event: ab\n"));
    }

    #[test]
    fn carriage_return_in_data_is_dropped() {
        let ev = SseEvent::data("a\rb\nc\rd");
        // \r is dropped within each line; \n still splits.
        assert_eq!(ev.to_wire(), "data: ab\ndata: cd\n\n");
    }

    #[tokio::test]
    async fn sse_response_sets_required_headers() {
        use futures::stream;
        let s = stream::iter(vec![SseEvent::data("hi")]);
        let res = SseResponse::new(s).into_response();
        assert_eq!(res.status(), http::StatusCode::OK);
        assert_eq!(
            res.headers().get(http::header::CONTENT_TYPE).unwrap(),
            "text/event-stream"
        );
        assert_eq!(
            res.headers().get(http::header::CACHE_CONTROL).unwrap(),
            "no-cache"
        );
        assert_eq!(
            res.headers().get(http::header::CONNECTION).unwrap(),
            "keep-alive"
        );
        assert_eq!(res.headers().get("x-accel-buffering").unwrap(), "no");
    }

    #[tokio::test]
    async fn sse_response_streams_event_bytes() {
        use futures::stream;
        use http_body_util::BodyExt;

        let s = stream::iter(vec![
            SseEvent::data("first"),
            SseEvent::data("second").with_event("update"),
        ]);
        let res = SseResponse::new(s).into_response();
        let collected = res.into_body().collect().await.unwrap().to_bytes();
        let text = std::str::from_utf8(&collected).unwrap();
        assert_eq!(text, "data: first\n\nevent: update\ndata: second\n\n");
    }

    #[tokio::test]
    async fn keep_alive_interleaves_pings() {
        use futures::StreamExt;
        use std::time::Duration;

        // Source emits one event then never completes.
        let pending = futures::stream::pending::<SseEvent>();
        let source = futures::stream::iter(vec![SseEvent::data("real")]).chain(pending);

        let mut combined = Box::pin(keep_alive(source, Duration::from_millis(20)));

        let first = tokio::time::timeout(Duration::from_millis(200), combined.next())
            .await
            .unwrap()
            .unwrap();
        // We can't guarantee ordering between the source event and the first
        // ping, but at least one of the first two items must be a ping.
        let second = tokio::time::timeout(Duration::from_millis(200), combined.next())
            .await
            .unwrap()
            .unwrap();
        let saw_ping = first.comment.is_some() || second.comment.is_some();
        assert!(saw_ping, "expected at least one keep-alive ping");
    }
}
