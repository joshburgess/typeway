//! Shared body type used throughout the server.
//!
//! [`BoxBody`] is a type-erased HTTP body that supports both buffered and
//! streaming responses. Handlers return `Response<BoxBody>`.

use bytes::Bytes;
use http_body_util::combinators::BoxBody as HBoxBody;
use http_body_util::{BodyExt, Empty, Full, StreamBody};

/// The response body type used by wayward handlers.
///
/// This is a type-erased body that can wrap buffered data (`Full<Bytes>`),
/// streaming data, or an empty body. It implements `http_body::Body`.
pub type BoxBody = HBoxBody<Bytes, BoxBodyError>;

/// Error type for the boxed body. Infallible for buffered bodies,
/// but allows streaming bodies to report errors.
pub type BoxBodyError = Box<dyn std::error::Error + Send + Sync>;

/// Create a `BoxBody` from bytes.
pub fn body_from_bytes(bytes: Bytes) -> BoxBody {
    Full::new(bytes).map_err(|e| match e {}).boxed()
}

/// Create a `BoxBody` from a string.
pub fn body_from_string(s: String) -> BoxBody {
    body_from_bytes(Bytes::from(s))
}

/// Create an empty `BoxBody`.
pub fn empty_body() -> BoxBody {
    Empty::new().map_err(|e| match e {}).boxed()
}

/// Create a streaming `BoxBody` from a `Stream` of `Result<Frame<Bytes>, E>`.
///
/// Use this for Server-Sent Events, chunked responses, or any streaming body.
///
/// # Example
///
/// ```ignore
/// use futures::stream;
/// use http_body::Frame;
///
/// let chunks = stream::iter(vec![
///     Ok(Frame::data(Bytes::from("chunk 1\n"))),
///     Ok(Frame::data(Bytes::from("chunk 2\n"))),
/// ]);
/// let body = body_from_stream(chunks);
/// ```
pub fn body_from_stream<S>(stream: S) -> BoxBody
where
    S: futures::Stream<Item = Result<http_body::Frame<Bytes>, BoxBodyError>>
        + Send
        + Sync
        + 'static,
{
    BoxBody::new(StreamBody::new(stream))
}

/// Create an SSE (Server-Sent Events) body from a stream of event strings.
///
/// Each string in the stream is formatted as an SSE event (`data: ...\n\n`).
///
/// # Example
///
/// ```ignore
/// use futures::stream;
///
/// let events = stream::iter(vec!["hello", "world"]);
/// let body = sse_body(events.map(|s| s.to_string()));
/// ```
pub fn sse_body<S>(stream: S) -> BoxBody
where
    S: futures::Stream<Item = String> + Send + Sync + 'static,
{
    use futures::StreamExt;
    let framed = stream.map(|event| {
        let data = format!("data: {event}\n\n");
        Ok(http_body::Frame::data(Bytes::from(data)))
    });
    body_from_stream(framed)
}
