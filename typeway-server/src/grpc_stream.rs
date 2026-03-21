//! Streaming gRPC response type for server-streaming RPCs.
//!
//! [`GrpcStream<T>`] is returned by handlers to stream individual items
//! to the client. Each item is serialized, gRPC-framed, and sent as a
//! separate HTTP/2 data frame. When the stream ends, gRPC trailers with
//! `grpc-status: 0` are appended.
//!
//! # Example
//!
//! ```ignore
//! use typeway_server::grpc_stream::{GrpcStream, GrpcStreamSender};
//!
//! async fn list_users(state: State<Db>) -> GrpcStream<User> {
//!     let (tx, stream) = GrpcStream::channel(32);
//!     tokio::spawn(async move {
//!         for user in state.db.all_users().await {
//!             if tx.send(user).await.is_err() { break; }
//!         }
//!     });
//!     stream
//! }
//! ```

use bytes::Bytes;
use http_body::Frame;
use serde::Serialize;

use typeway_grpc::framing::encode_grpc_frame;
use typeway_grpc::status::{GrpcCode, GrpcStatus};

use crate::body::{body_from_stream, BoxBody, BoxBodyError};
use crate::response::IntoResponse;

/// Marker extension inserted into the response by `GrpcStream::into_response`.
///
/// The native gRPC dispatch checks for this to know the response body
/// is already gRPC-framed with trailers — it should be passed through
/// without re-wrapping.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GrpcStreamMarker;

/// A sender handle for streaming gRPC responses.
///
/// Send items through this handle. The framework serializes each item
/// as JSON, wraps it in gRPC framing, and streams it to the client.
///
/// Dropping this handle signals the end of the stream.
pub struct GrpcStreamSender<T> {
    tx: tokio::sync::mpsc::Sender<Result<T, GrpcStatus>>,
}

impl<T> GrpcStreamSender<T> {
    /// Send an item to the stream.
    ///
    /// Returns `Err` if the receiver has been dropped (client disconnected).
    pub async fn send(&self, item: T) -> Result<(), typeway_grpc::StreamSendError> {
        self.tx
            .send(Ok(item))
            .await
            .map_err(|_| typeway_grpc::StreamSendError)
    }

    /// Send an error status, ending the stream with that status.
    pub async fn send_error(&self, status: GrpcStatus) -> Result<(), typeway_grpc::StreamSendError> {
        self.tx
            .send(Err(status))
            .await
            .map_err(|_| typeway_grpc::StreamSendError)
    }
}

/// A streaming gRPC response.
///
/// Returned by handlers for server-streaming RPCs. The framework reads
/// items from the internal channel, serializes each as JSON, wraps in
/// gRPC framing, and streams them to the client. When the channel
/// closes, a trailers frame with `grpc-status: 0` (OK) is sent.
///
/// # Example
///
/// ```ignore
/// async fn list_users(state: State<Db>) -> GrpcStream<User> {
///     let (tx, stream) = GrpcStream::channel(32);
///     tokio::spawn(async move {
///         for user in state.db.all_users().await {
///             if tx.send(user).await.is_err() { break; }
///         }
///     });
///     stream
/// }
/// ```
pub struct GrpcStream<T> {
    rx: tokio::sync::mpsc::Receiver<Result<T, GrpcStatus>>,
}

impl<T> GrpcStream<T> {
    /// Create a sender/stream pair with the given buffer size.
    ///
    /// The `buffer` parameter controls backpressure: when the buffer is
    /// full, `GrpcStreamSender::send` will wait until the client reads.
    pub fn channel(buffer: usize) -> (GrpcStreamSender<T>, GrpcStream<T>) {
        let (tx, rx) = tokio::sync::mpsc::channel(buffer);
        (GrpcStreamSender { tx }, GrpcStream { rx })
    }
}

/// State for the streaming body unfold.
struct StreamState<T> {
    rx: tokio::sync::mpsc::Receiver<Result<T, GrpcStatus>>,
    done: bool,
}

impl<T: Serialize + Send + 'static> IntoResponse for GrpcStream<T> {
    fn into_response(self) -> http::Response<BoxBody> {
        let state = StreamState {
            rx: self.rx,
            done: false,
        };

        // Use futures::stream::unfold to create a Stream from the receiver.
        // Each step yields a gRPC-framed data frame or a trailers frame.
        let stream = futures::stream::unfold(state, |mut state| async move {
            if state.done {
                return None;
            }

            match state.rx.recv().await {
                Some(Ok(item)) => {
                    let json_bytes = serde_json::to_vec(&item).unwrap_or_default();
                    let framed = encode_grpc_frame(&json_bytes);
                    let frame: Result<Frame<Bytes>, BoxBodyError> =
                        Ok(Frame::data(Bytes::from(framed)));
                    Some((frame, state))
                }
                Some(Err(status)) => {
                    // Error from handler — send trailers with error status.
                    state.done = true;
                    let trailers = build_trailers(&status);
                    Some((Ok(Frame::trailers(trailers)), state))
                }
                None => {
                    // Channel closed — send OK trailers.
                    state.done = true;
                    let ok_status = GrpcStatus {
                        code: GrpcCode::Ok,
                        message: String::new(),
                    };
                    let trailers = build_trailers(&ok_status);
                    Some((Ok(Frame::trailers(trailers)), state))
                }
            }
        });

        let body = body_from_stream(stream);

        let mut res = http::Response::new(body);
        *res.status_mut() = http::StatusCode::OK;
        res.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("application/grpc+json"),
        );
        // Mark this response so the native dispatch passes it through.
        res.extensions_mut().insert(GrpcStreamMarker);
        res
    }
}

/// Build a trailers HeaderMap from a GrpcStatus.
fn build_trailers(status: &GrpcStatus) -> http::HeaderMap {
    let mut trailers = http::HeaderMap::new();
    trailers.insert(
        "grpc-status",
        status
            .code
            .as_i32()
            .to_string()
            .parse()
            .expect("valid grpc-status"),
    );
    if !status.message.is_empty() {
        if let Ok(val) = status.message.parse() {
            trailers.insert("grpc-message", val);
        }
    }
    trailers
}
