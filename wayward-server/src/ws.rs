//! WebSocket upgrade support.
//!
//! Provides a [`WebSocketUpgrade`] extractor that handles the HTTP upgrade
//! handshake and gives you a tokio-tungstenite WebSocket connection.
//!
//! # Example
//!
//! ```ignore
//! use wayward_server::ws::WebSocketUpgrade;
//! use futures::{SinkExt, StreamExt};
//!
//! async fn ws_handler(upgrade: WebSocketUpgrade) -> http::Response<BoxBody> {
//!     upgrade.on_upgrade(|mut ws| async move {
//!         while let Some(Ok(msg)) = ws.next().await {
//!             if msg.is_text() {
//!                 let _ = ws.send(msg).await;
//!             }
//!         }
//!     })
//! }
//! ```

use std::future::Future;

use http::StatusCode;
use hyper::upgrade::OnUpgrade;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::WebSocketStream;

use crate::body::{empty_body, BoxBody};
use crate::extract::FromRequestParts;

/// WebSocket upgrade extractor.
///
/// When used as a handler argument, performs the HTTP upgrade handshake.
/// Call [`on_upgrade`](WebSocketUpgrade::on_upgrade) with a callback
/// to handle the WebSocket connection.
pub struct WebSocketUpgrade {
    on_upgrade: OnUpgrade,
    sec_websocket_key: String,
}

impl FromRequestParts for WebSocketUpgrade {
    type Error = (StatusCode, String);

    fn from_request_parts(parts: &http::request::Parts) -> Result<Self, Self::Error> {
        // Verify this is a valid WebSocket upgrade request.
        let is_upgrade = parts
            .headers
            .get(http::header::CONNECTION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.to_lowercase().contains("upgrade"));

        let is_websocket = parts
            .headers
            .get(http::header::UPGRADE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.to_lowercase() == "websocket");

        if !is_upgrade || !is_websocket {
            return Err((
                StatusCode::BAD_REQUEST,
                "not a valid WebSocket upgrade request".to_string(),
            ));
        }

        let key = parts
            .headers
            .get("sec-websocket-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "missing Sec-WebSocket-Key header".to_string(),
                )
            })?
            .to_string();

        let on_upgrade = parts
            .extensions
            .get::<OnUpgrade>()
            .cloned()
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "upgrade not available — is this a hyper connection?".to_string(),
                )
            })?;

        Ok(WebSocketUpgrade {
            on_upgrade,
            sec_websocket_key: key,
        })
    }
}

impl WebSocketUpgrade {
    /// Complete the upgrade and spawn the WebSocket handler.
    ///
    /// Returns the `101 Switching Protocols` response. The `callback` receives
    /// a `WebSocketStream` after the upgrade completes.
    pub fn on_upgrade<F, Fut>(self, callback: F) -> http::Response<BoxBody>
    where
        F: FnOnce(WebSocketStream<hyper_util::rt::TokioIo<hyper::upgrade::Upgraded>>) -> Fut
            + Send
            + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let accept_key = tungstenite_accept_key(&self.sec_websocket_key);

        // Spawn the upgrade handler.
        tokio::spawn(async move {
            match self.on_upgrade.await {
                Ok(upgraded) => {
                    let io = hyper_util::rt::TokioIo::new(upgraded);
                    let ws = WebSocketStream::from_raw_socket(io, Role::Server, None).await;
                    callback(ws).await;
                }
                Err(e) => {
                    eprintln!("WebSocket upgrade failed: {e}");
                }
            }
        });

        // Return 101 Switching Protocols.
        let mut res = http::Response::new(empty_body());
        *res.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
        res.headers_mut().insert(
            http::header::CONNECTION,
            http::HeaderValue::from_static("upgrade"),
        );
        res.headers_mut().insert(
            http::header::UPGRADE,
            http::HeaderValue::from_static("websocket"),
        );
        if let Ok(val) = http::HeaderValue::from_str(&accept_key) {
            res.headers_mut().insert("sec-websocket-accept", val);
        }
        res
    }
}

/// Compute the Sec-WebSocket-Accept key per RFC 6455.
fn tungstenite_accept_key(key: &str) -> String {
    let mut hasher = sha1_smol::Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-5AB5DC11CE56");
    base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        hasher.digest().bytes(),
    )
}
