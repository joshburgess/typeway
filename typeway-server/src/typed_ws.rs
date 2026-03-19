//! Session-typed WebSocket channels.
//!
//! Wraps a raw `WebSocketStream` in a [`TypedWebSocket<S>`] that tracks the
//! protocol state `S` at the type level. Each operation (send, receive, offer,
//! select) consumes the channel and returns it in the next protocol state,
//! so Rust's ownership system enforces that messages are exchanged in the
//! correct order.
//!
//! # Example
//!
//! ```ignore
//! use typeway_core::session::*;
//! use typeway_server::typed_ws::TypedWebSocket;
//!
//! // Protocol: send greeting, receive name, send welcome, end.
//! type Greet = Send<String, Recv<String, Send<String, End>>>;
//!
//! async fn greet_handler(ws: TypedWebSocket<Greet>) {
//!     let ws = ws.send("Hello! What is your name?".to_string()).await.unwrap();
//!     let (name, ws) = ws.recv().await.unwrap();
//!     let ws = ws.send(format!("Welcome, {name}!")).await.unwrap();
//!     ws.close().await.unwrap();
//! }
//! ```

use std::fmt;
use std::marker::PhantomData;

use futures::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::WebSocketStream;

use typeway_core::session::*;

/// A typed WebSocket channel at protocol state `S`.
///
/// Each operation consumes `self` and returns a new channel at the next
/// state, enforcing protocol ordering via Rust's move semantics.
pub struct TypedWebSocket<S: SessionType> {
    inner: WebSocketStream<hyper_util::rt::TokioIo<hyper::upgrade::Upgraded>>,
    _state: PhantomData<S>,
}

impl<S: SessionType> TypedWebSocket<S> {
    /// Wrap a raw `WebSocketStream` in a typed channel at state `S`.
    ///
    /// The caller is responsible for choosing the correct initial protocol
    /// state. Typically this is called by [`WebSocketUpgrade::on_upgrade_typed`].
    pub fn new(
        inner: WebSocketStream<hyper_util::rt::TokioIo<hyper::upgrade::Upgraded>>,
    ) -> Self {
        TypedWebSocket {
            inner,
            _state: PhantomData,
        }
    }
}

impl<T: Serialize, Next: SessionType> TypedWebSocket<Send<T, Next>> {
    /// Send a message of type `T`. Returns the channel in the next protocol state.
    pub async fn send(mut self, msg: T) -> Result<TypedWebSocket<Next>, WebSocketError> {
        let json = serde_json::to_string(&msg).map_err(WebSocketError::Serialize)?;
        self.inner
            .send(tungstenite::Message::Text(json.into()))
            .await
            .map_err(WebSocketError::Transport)?;
        Ok(TypedWebSocket {
            inner: self.inner,
            _state: PhantomData,
        })
    }
}

impl<T: DeserializeOwned, Next: SessionType> TypedWebSocket<Recv<T, Next>> {
    /// Receive a message of type `T`. Returns the message and the channel
    /// in the next protocol state.
    pub async fn recv(mut self) -> Result<(T, TypedWebSocket<Next>), WebSocketError> {
        loop {
            match self.inner.next().await {
                Some(Ok(msg)) if msg.is_text() => {
                    let text = msg
                        .into_text()
                        .map_err(|_| WebSocketError::Protocol("expected text frame".into()))?;
                    let val: T =
                        serde_json::from_str(&text).map_err(WebSocketError::Deserialize)?;
                    return Ok((
                        val,
                        TypedWebSocket {
                            inner: self.inner,
                            _state: PhantomData,
                        },
                    ));
                }
                Some(Ok(msg)) if msg.is_close() => return Err(WebSocketError::Closed),
                Some(Ok(_)) => continue, // skip ping/pong
                Some(Err(e)) => return Err(WebSocketError::Transport(e)),
                None => return Err(WebSocketError::Closed),
            }
        }
    }
}

impl TypedWebSocket<End> {
    /// Close the WebSocket connection. This is the only operation available
    /// at protocol termination.
    pub async fn close(mut self) -> Result<(), WebSocketError> {
        self.inner
            .close(None)
            .await
            .map_err(WebSocketError::Transport)
    }
}

impl<L: SessionType, R: SessionType> TypedWebSocket<Offer<L, R>> {
    /// Wait for the remote peer's branch choice.
    ///
    /// Returns [`Either::Left`] or [`Either::Right`] depending on the
    /// selection message received.
    pub async fn offer(
        mut self,
    ) -> Result<Either<TypedWebSocket<L>, TypedWebSocket<R>>, WebSocketError> {
        loop {
            match self.inner.next().await {
                Some(Ok(msg)) if msg.is_text() => {
                    let text = msg
                        .into_text()
                        .map_err(|_| WebSocketError::Protocol("expected text frame".into()))?;
                    if text.contains("\"branch\":\"L\"") || text.contains("\"branch\":\"left\"") {
                        return Ok(Either::Left(TypedWebSocket {
                            inner: self.inner,
                            _state: PhantomData,
                        }));
                    } else {
                        return Ok(Either::Right(TypedWebSocket {
                            inner: self.inner,
                            _state: PhantomData,
                        }));
                    }
                }
                Some(Ok(msg)) if msg.is_close() => return Err(WebSocketError::Closed),
                Some(Ok(_)) => continue,
                Some(Err(e)) => return Err(WebSocketError::Transport(e)),
                None => return Err(WebSocketError::Closed),
            }
        }
    }
}

impl<L: SessionType, R: SessionType> TypedWebSocket<Select<L, R>> {
    /// Choose the left branch of the protocol.
    pub async fn select_left(mut self) -> Result<TypedWebSocket<L>, WebSocketError> {
        self.inner
            .send(tungstenite::Message::Text("{\"branch\":\"L\"}".into()))
            .await
            .map_err(WebSocketError::Transport)?;
        Ok(TypedWebSocket {
            inner: self.inner,
            _state: PhantomData,
        })
    }

    /// Choose the right branch of the protocol.
    pub async fn select_right(mut self) -> Result<TypedWebSocket<R>, WebSocketError> {
        self.inner
            .send(tungstenite::Message::Text("{\"branch\":\"R\"}".into()))
            .await
            .map_err(WebSocketError::Transport)?;
        Ok(TypedWebSocket {
            inner: self.inner,
            _state: PhantomData,
        })
    }
}

impl<B: SessionType> TypedWebSocket<Rec<B>> {
    /// Enter the recursive protocol body.
    ///
    /// This is a zero-cost state transition that unwraps the `Rec` marker.
    pub fn enter(self) -> TypedWebSocket<B> {
        TypedWebSocket {
            inner: self.inner,
            _state: PhantomData,
        }
    }
}

impl TypedWebSocket<Var> {
    /// Loop back to the enclosing [`Rec`].
    ///
    /// The caller must specify the `Rec` body type `B` since `Var` does not
    /// carry it. This is typically inferred from context.
    pub fn recurse<B: SessionType>(self) -> TypedWebSocket<Rec<B>> {
        TypedWebSocket {
            inner: self.inner,
            _state: PhantomData,
        }
    }
}

/// Branch result for [`Offer`].
pub enum Either<L, R> {
    /// The remote peer chose the left branch.
    Left(L),
    /// The remote peer chose the right branch.
    Right(R),
}

/// Errors arising from typed WebSocket operations.
#[derive(Debug)]
pub enum WebSocketError {
    /// Underlying transport error from tungstenite.
    Transport(tungstenite::Error),
    /// Failed to serialize an outgoing message.
    Serialize(serde_json::Error),
    /// Failed to deserialize an incoming message.
    Deserialize(serde_json::Error),
    /// Protocol-level error (unexpected frame type, etc.).
    Protocol(String),
    /// The connection was closed unexpectedly.
    Closed,
}

impl fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebSocketError::Transport(e) => write!(f, "transport error: {e}"),
            WebSocketError::Serialize(e) => write!(f, "serialization error: {e}"),
            WebSocketError::Deserialize(e) => write!(f, "deserialization error: {e}"),
            WebSocketError::Protocol(msg) => write!(f, "protocol error: {msg}"),
            WebSocketError::Closed => write!(f, "connection closed"),
        }
    }
}

impl std::error::Error for WebSocketError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WebSocketError::Transport(e) => Some(e),
            WebSocketError::Serialize(e) => Some(e),
            WebSocketError::Deserialize(e) => Some(e),
            _ => None,
        }
    }
}
