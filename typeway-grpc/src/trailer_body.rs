//! HTTP/2 trailer-aware body types for native gRPC responses.
//!
//! gRPC uses HTTP/2 trailers to convey status information (`grpc-status`,
//! `grpc-message`). These body types yield data frames followed by a
//! trailers frame, replacing the bridge's approach of putting status
//! in response headers.
//!
//! Two variants:
//! - [`GrpcBody`] — unary responses (single data frame + trailers)
//! - [`GrpcStreamBody`] — streaming responses (multiple frames from a channel + trailers)

use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::HeaderMap;
use http_body::Frame;
use pin_project_lite::pin_project;

use crate::status::{encode_grpc_message, GrpcCode, GrpcStatus};

/// Build a trailers `HeaderMap` from a [`GrpcStatus`].
fn status_to_trailers(status: &GrpcStatus) -> HeaderMap {
    let mut trailers = HeaderMap::new();
    trailers.insert(
        "grpc-status",
        status
            .code
            .as_i32()
            .to_string()
            .parse()
            .expect("valid grpc-status value"),
    );
    if !status.message.is_empty() {
        let encoded = encode_grpc_message(&status.message);
        if let Ok(val) = encoded.parse() {
            trailers.insert("grpc-message", val);
        }
    }
    trailers
}

pin_project! {
    /// A gRPC response body for unary RPCs.
    ///
    /// Yields a single data frame (the gRPC-framed response message)
    /// followed by an HTTP/2 trailers frame with `grpc-status` and
    /// `grpc-message`.
    pub struct GrpcBody {
        data: Option<Bytes>,
        trailers: Option<HeaderMap>,
    }
}

impl GrpcBody {
    /// Create a body with data and `grpc-status: 0` (OK) trailers.
    pub fn ok(data: Bytes) -> Self {
        let trailers = status_to_trailers(&GrpcStatus {
            code: GrpcCode::Ok,
            message: String::new(),
        });
        GrpcBody {
            data: Some(data),
            trailers: Some(trailers),
        }
    }

    /// Create a body with data and a specific gRPC status in trailers.
    pub fn with_status(data: Bytes, status: GrpcStatus) -> Self {
        let trailers = status_to_trailers(&status);
        GrpcBody {
            data: Some(data),
            trailers: Some(trailers),
        }
    }

    /// Create an empty body with error trailers (no data frames).
    pub fn error(status: GrpcStatus) -> Self {
        let trailers = status_to_trailers(&status);
        GrpcBody {
            data: None,
            trailers: Some(trailers),
        }
    }
}

impl http_body::Body for GrpcBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();

        // Yield the data frame first (if any).
        if let Some(data) = this.data.take() {
            if !data.is_empty() {
                return Poll::Ready(Some(Ok(Frame::data(data))));
            }
        }

        // Then yield the trailers frame.
        if let Some(trailers) = this.trailers.take() {
            return Poll::Ready(Some(Ok(Frame::trailers(trailers))));
        }

        // Done.
        Poll::Ready(None)
    }

    fn is_end_stream(&self) -> bool {
        self.data.is_none() && self.trailers.is_none()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        match &self.data {
            Some(data) => http_body::SizeHint::with_exact(data.len() as u64),
            None => http_body::SizeHint::with_exact(0),
        }
    }
}

pin_project! {
    /// A gRPC response body for server-streaming RPCs.
    ///
    /// Reads gRPC-framed messages from a `tokio::sync::mpsc::Receiver`
    /// and yields them as data frames. When the channel closes, yields
    /// a trailers frame with the final gRPC status.
    pub struct GrpcStreamBody {
        #[pin]
        receiver: tokio::sync::mpsc::Receiver<Bytes>,
        status: GrpcStatus,
        done: bool,
    }
}

impl GrpcStreamBody {
    /// Create a streaming body from a channel receiver.
    ///
    /// Each item received is expected to be a gRPC-framed message
    /// (5-byte header + payload). When the sender drops or the channel
    /// closes, a trailers frame with `grpc-status: 0` (OK) is sent.
    pub fn new(receiver: tokio::sync::mpsc::Receiver<Bytes>) -> Self {
        GrpcStreamBody {
            receiver,
            status: GrpcStatus {
                code: GrpcCode::Ok,
                message: String::new(),
            },
            done: false,
        }
    }

    /// Create a streaming body with a custom final status.
    pub fn with_status(receiver: tokio::sync::mpsc::Receiver<Bytes>, status: GrpcStatus) -> Self {
        GrpcStreamBody {
            receiver,
            status,
            done: false,
        }
    }
}

impl http_body::Body for GrpcStreamBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let mut this = self.project();

        if *this.done {
            return Poll::Ready(None);
        }

        match this.receiver.as_mut().poll_recv(cx) {
            Poll::Ready(Some(data)) => Poll::Ready(Some(Ok(Frame::data(data)))),
            Poll::Ready(None) => {
                // Channel closed — send trailers.
                *this.done = true;
                let trailers = status_to_trailers(this.status);
                Poll::Ready(Some(Ok(Frame::trailers(trailers))))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body::Body;

    #[tokio::test]
    async fn grpc_body_ok_yields_data_then_trailers() {
        let data = Bytes::from_static(b"\x00\x00\x00\x00\x05hello");
        let mut body = GrpcBody::ok(data.clone());

        // First poll: data frame.
        let frame = Pin::new(&mut body)
            .poll_frame(&mut Context::from_waker(futures::task::noop_waker_ref()))
            .map(|opt| opt.map(|r| r.unwrap()));
        match frame {
            Poll::Ready(Some(frame)) => {
                assert!(frame.is_data());
                assert_eq!(frame.into_data().unwrap(), data);
            }
            other => panic!("expected data frame, got {other:?}"),
        }

        // Second poll: trailers frame.
        let frame = Pin::new(&mut body)
            .poll_frame(&mut Context::from_waker(futures::task::noop_waker_ref()))
            .map(|opt| opt.map(|r| r.unwrap()));
        match frame {
            Poll::Ready(Some(frame)) => {
                assert!(frame.is_trailers());
                let trailers = frame.into_trailers().unwrap();
                assert_eq!(trailers.get("grpc-status").unwrap(), "0");
            }
            other => panic!("expected trailers frame, got {other:?}"),
        }

        // Third poll: done.
        let frame = Pin::new(&mut body)
            .poll_frame(&mut Context::from_waker(futures::task::noop_waker_ref()));
        assert!(matches!(frame, Poll::Ready(None)));
    }

    #[tokio::test]
    async fn grpc_body_error_yields_only_trailers() {
        let status = GrpcStatus {
            code: GrpcCode::NotFound,
            message: "user not found".to_string(),
        };
        let mut body = GrpcBody::error(status);

        // First poll: trailers (no data frame for errors).
        let frame = Pin::new(&mut body)
            .poll_frame(&mut Context::from_waker(futures::task::noop_waker_ref()))
            .map(|opt| opt.map(|r| r.unwrap()));
        match frame {
            Poll::Ready(Some(frame)) => {
                assert!(frame.is_trailers());
                let trailers = frame.into_trailers().unwrap();
                assert_eq!(trailers.get("grpc-status").unwrap(), "5");
                assert_eq!(trailers.get("grpc-message").unwrap(), "user not found");
            }
            other => panic!("expected trailers frame, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn grpc_stream_body_yields_frames_then_trailers() {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let mut body = GrpcStreamBody::new(rx);

        // Send two frames.
        tx.send(Bytes::from_static(b"\x00\x00\x00\x00\x01a"))
            .await
            .unwrap();
        tx.send(Bytes::from_static(b"\x00\x00\x00\x00\x01b"))
            .await
            .unwrap();
        drop(tx);

        // Collect all frames.
        let mut data_frames = Vec::new();
        let mut trailers = None;
        loop {
            let frame = Pin::new(&mut body)
                .poll_frame(&mut Context::from_waker(futures::task::noop_waker_ref()));
            match frame {
                Poll::Ready(Some(Ok(f))) => {
                    if f.is_data() {
                        data_frames.push(f.into_data().unwrap());
                    } else if f.is_trailers() {
                        trailers = Some(f.into_trailers().unwrap());
                        break;
                    }
                }
                Poll::Ready(None) => break,
                Poll::Pending => break,
                Poll::Ready(Some(Err(e))) => match e {},
            }
        }

        assert_eq!(data_frames.len(), 2);
        assert!(trailers.is_some());
        let trailers = trailers.unwrap();
        assert_eq!(trailers.get("grpc-status").unwrap(), "0");
    }
}
