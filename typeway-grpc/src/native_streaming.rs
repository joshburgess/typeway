//! Real streaming types for native gRPC dispatch.
//!
//! These types back the streaming markers in [`crate::streaming`] with
//! actual `tokio::sync::mpsc` channels. They provide typed send/receive
//! handles for server-streaming, client-streaming, and bidirectional RPCs.
//!
//! The existing [`ServerStream`](crate::streaming::ServerStream),
//! [`ClientStream`](crate::streaming::ClientStream), and
//! [`BidirectionalStream`](crate::streaming::BidirectionalStream) remain
//! as type-level markers for proto generation. These runtime types are
//! the concrete handles that handlers interact with.

use crate::status::GrpcStatus;

/// Default channel buffer size for gRPC streaming.
pub const DEFAULT_STREAM_BUFFER: usize = 32;

/// Error returned when sending a message into a closed stream.
#[derive(Debug)]
pub struct StreamSendError;

impl std::fmt::Display for StreamSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "gRPC stream receiver dropped")
    }
}

impl std::error::Error for StreamSendError {}

/// A sender handle for server-streaming gRPC responses.
///
/// The handler sends items through this handle. The framework reads
/// from the corresponding receiver and encodes each item as a gRPC frame.
///
/// Dropping this handle signals the end of the stream.
pub struct GrpcSender<T> {
    tx: tokio::sync::mpsc::Sender<Result<T, GrpcStatus>>,
}

impl<T> GrpcSender<T> {
    /// Send a message to the stream.
    ///
    /// Returns `Err` if the receiver has been dropped (client disconnected).
    pub async fn send(&self, item: T) -> Result<(), StreamSendError> {
        self.tx
            .send(Ok(item))
            .await
            .map_err(|_| StreamSendError)
    }

    /// Send an error to the stream.
    ///
    /// The error will be conveyed as a gRPC status to the client.
    /// After sending an error, the stream should be considered finished.
    pub async fn send_error(&self, status: GrpcStatus) -> Result<(), StreamSendError> {
        self.tx
            .send(Err(status))
            .await
            .map_err(|_| StreamSendError)
    }
}

/// A receiver handle for client-streaming gRPC requests.
///
/// The handler reads incoming messages through this handle. When the
/// client closes the stream, `recv()` returns `None`.
pub struct GrpcReceiver<T> {
    rx: tokio::sync::mpsc::Receiver<Result<T, GrpcStatus>>,
}

impl<T> GrpcReceiver<T> {
    /// Receive the next message from the stream.
    ///
    /// Returns:
    /// - `Some(Ok(item))` — a message was received
    /// - `Some(Err(status))` — the client sent an error
    /// - `None` — the stream has ended (client closed)
    pub async fn recv(&mut self) -> Option<Result<T, GrpcStatus>> {
        self.rx.recv().await
    }

    /// Collect all messages into a `Vec`.
    ///
    /// Returns an error if any message in the stream is an error.
    pub async fn collect(mut self) -> Result<Vec<T>, GrpcStatus> {
        let mut items = Vec::new();
        while let Some(result) = self.rx.recv().await {
            items.push(result?);
        }
        Ok(items)
    }

    /// Collect all messages, returning at most `limit` items.
    ///
    /// Stops reading after `limit` items are collected, even if the
    /// stream has more. Returns an error if any received message is
    /// an error.
    pub async fn collect_limit(mut self, limit: usize) -> Result<Vec<T>, GrpcStatus> {
        let mut items = Vec::with_capacity(limit.min(64));
        while items.len() < limit {
            match self.rx.recv().await {
                Some(Ok(item)) => items.push(item),
                Some(Err(status)) => return Err(status),
                None => break,
            }
        }
        Ok(items)
    }
}

/// A bidirectional stream handle for bidirectional-streaming RPCs.
///
/// Provides both a sender (for server → client messages) and a receiver
/// (for client → server messages).
pub struct GrpcBiStream<Req, Resp> {
    /// Receiver for incoming client messages.
    pub rx: GrpcReceiver<Req>,
    /// Sender for outgoing server responses.
    pub tx: GrpcSender<Resp>,
}

impl<Req, Resp> GrpcBiStream<Req, Resp> {
    /// Receive the next message from the client.
    pub async fn recv(&mut self) -> Option<Result<Req, GrpcStatus>> {
        self.rx.recv().await
    }

    /// Send a message to the client.
    pub async fn send(&self, item: Resp) -> Result<(), StreamSendError> {
        self.tx.send(item).await
    }

    /// Send an error status to the client.
    pub async fn send_error(&self, status: GrpcStatus) -> Result<(), StreamSendError> {
        self.tx.send_error(status).await
    }
}

/// Create a sender/receiver pair for gRPC streaming.
///
/// The `buffer` parameter controls backpressure: when the buffer is full,
/// `GrpcSender::send` will wait until the receiver reads a message.
pub fn grpc_channel<T>(buffer: usize) -> (GrpcSender<T>, GrpcReceiver<T>) {
    let (tx, rx) = tokio::sync::mpsc::channel(buffer);
    (GrpcSender { tx }, GrpcReceiver { rx })
}

/// Create a sender/receiver pair with the default buffer size.
pub fn grpc_channel_default<T>() -> (GrpcSender<T>, GrpcReceiver<T>) {
    grpc_channel(DEFAULT_STREAM_BUFFER)
}

/// Create the channel pair for a bidirectional stream.
///
/// Returns `(server_bistream, client_tx, client_rx)` where:
/// - `server_bistream` is the handle the server handler receives
/// - `client_tx` feeds the server's receiver (framework reads client frames into this)
/// - `client_rx` reads the server's sender (framework writes to client from this)
pub fn grpc_bidi_channel<Req, Resp>(
    buffer: usize,
) -> (
    GrpcBiStream<Req, Resp>,
    tokio::sync::mpsc::Sender<Result<Req, GrpcStatus>>,
    tokio::sync::mpsc::Receiver<Result<Resp, GrpcStatus>>,
) {
    let (client_to_server_tx, client_to_server_rx) = tokio::sync::mpsc::channel(buffer);
    let (server_to_client_tx, server_to_client_rx) = tokio::sync::mpsc::channel(buffer);

    let bistream = GrpcBiStream {
        rx: GrpcReceiver {
            rx: client_to_server_rx,
        },
        tx: GrpcSender {
            tx: server_to_client_tx,
        },
    };

    (bistream, client_to_server_tx, server_to_client_rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sender_receiver_roundtrip() {
        let (tx, mut rx) = grpc_channel::<String>(8);

        tx.send("hello".to_string()).await.unwrap();
        tx.send("world".to_string()).await.unwrap();
        drop(tx);

        assert_eq!(rx.recv().await.unwrap().unwrap(), "hello");
        assert_eq!(rx.recv().await.unwrap().unwrap(), "world");
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn receiver_collect() {
        let (tx, rx) = grpc_channel::<u32>(8);

        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        tx.send(3).await.unwrap();
        drop(tx);

        let items = rx.collect().await.unwrap();
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn receiver_collect_with_error() {
        use crate::GrpcCode;

        let (tx, rx) = grpc_channel::<u32>(8);

        tx.send(1).await.unwrap();
        tx.send_error(GrpcStatus {
            code: GrpcCode::Internal,
            message: "oops".to_string(),
        })
        .await
        .unwrap();
        drop(tx);

        let err = rx.collect().await.unwrap_err();
        assert_eq!(err.code, GrpcCode::Internal);
    }

    #[tokio::test]
    async fn receiver_collect_limit() {
        let (tx, rx) = grpc_channel::<u32>(16);

        for i in 0..10 {
            tx.send(i).await.unwrap();
        }
        drop(tx);

        let items = rx.collect_limit(3).await.unwrap();
        assert_eq!(items, vec![0, 1, 2]);
    }

    #[tokio::test]
    async fn bistream_send_and_receive() {
        let (mut bistream, client_tx, mut client_rx) = grpc_bidi_channel::<String, String>(8);

        // Client sends to server.
        client_tx.send(Ok("from client".to_string())).await.unwrap();
        let msg = bistream.recv().await.unwrap().unwrap();
        assert_eq!(msg, "from client");

        // Server sends to client.
        bistream.send("from server".to_string()).await.unwrap();
        let msg = client_rx.recv().await.unwrap().unwrap();
        assert_eq!(msg, "from server");
    }

    #[tokio::test]
    async fn sender_error_on_dropped_receiver() {
        let (tx, rx) = grpc_channel::<u32>(1);
        drop(rx);
        assert!(tx.send(42).await.is_err());
    }
}
