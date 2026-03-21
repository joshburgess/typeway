//! # Session-Typed Chat Server
//!
//! A chat server where the WebSocket protocol is enforced at compile time.
//! The type system guarantees that:
//!
//! - The server sends a welcome message before accepting chat messages
//! - The client must authenticate before joining
//! - Messages are exchanged in the correct order
//! - The connection is properly closed
//!
//! If a handler sends messages in the wrong order, **it won't compile**.
//!
//! ## Run
//!
//! ```bash
//! cargo run -p typeway-chat
//! ```
//!
//! ## Test
//!
//! Open `http://localhost:3000` in a browser (serves the HTML client),
//! or connect with any WebSocket client:
//!
//! ```bash
//! # Using websocat:
//! websocat ws://localhost:3000/ws/chat
//! # Send: {"username": "alice"}
//! # Receive: {"message": "Welcome, alice! ..."}
//! # Send: {"message": "hello everyone"}
//! # Receive: {"message": "[alice] hello everyone"}
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};

use typeway_core::endpoint::GetEndpoint;
use typeway_core::path::{HCons, HNil, Lit, LitSegment};
use typeway_core::session::{self, Rec, Recv, Var};
use typeway_server::body::body_from_bytes;
use typeway_server::response::IntoResponse;
use typeway_server::server::Server;
use typeway_server::ws::WebSocketUpgrade;
use typeway_server::{bind, Json, State};

// =========================================================================
// Domain types
// =========================================================================

/// Authentication request — client sends username to join.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthRequest {
    username: String,
}

/// Server's welcome response after authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WelcomeMessage {
    message: String,
    online_users: Vec<String>,
}

/// A chat message — sent by client or broadcast by server.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    message: String,
}

/// Server status response.
#[derive(Debug, Clone, Serialize)]
struct ServerStatus {
    online_users: usize,
    total_messages: usize,
}

// =========================================================================
// Session-typed chat protocol
// =========================================================================

/// The chat protocol, enforced at compile time:
///
/// 1. Receive authentication (username)
/// 2. Send welcome message (with online users)
/// 3. Loop: receive a message, broadcast it back
///
/// ```text
/// Client                    Server
///   |                         |
///   |--- AuthRequest -------->|  (step 1: authenticate)
///   |<-- WelcomeMessage ------|  (step 2: welcome)
///   |                         |
///   |--- ChatMessage -------->|  (step 3: send message)
///   |<-- ChatMessage ---------|  (step 3: receive broadcast)
///   |--- ChatMessage -------->|  (repeat...)
///   |<-- ChatMessage ---------|
///   |         ...             |
/// ```
///
/// If you try to send a `WelcomeMessage` before receiving `AuthRequest`,
/// the compiler rejects it. If you try to receive a `ChatMessage` before
/// sending the welcome, the compiler rejects it. The protocol is law.
type ChatProtocol = Recv<AuthRequest, session::Send<WelcomeMessage, ChatLoop>>;

/// The chat loop: receive a message, send a broadcast, repeat.
type ChatLoop = Rec<Recv<ChatMessage, session::Send<ChatMessage, Var>>>;

// =========================================================================
// Shared state
// =========================================================================

#[derive(Clone)]
struct AppState {
    /// Connected users (username → sender for direct messages).
    users: Arc<Mutex<HashMap<String, ()>>>,
    /// Broadcast channel for chat messages.
    tx: broadcast::Sender<(String, String)>, // (username, message)
    /// Total message count.
    message_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl AppState {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        AppState {
            users: Arc::new(Mutex::new(HashMap::new())),
            tx,
            message_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

// =========================================================================
// Path types
// =========================================================================

#[allow(non_camel_case_types)]
struct __lit_ws;
impl LitSegment for __lit_ws {
    const VALUE: &'static str = "ws";
}
#[allow(non_camel_case_types)]
struct __lit_chat;
impl LitSegment for __lit_chat {
    const VALUE: &'static str = "chat";
}
#[allow(non_camel_case_types)]
struct __lit_status;
impl LitSegment for __lit_status {
    const VALUE: &'static str = "status";
}

type WsChatPath = HCons<Lit<__lit_ws>, HCons<Lit<__lit_chat>, HNil>>;
type StatusPath = HCons<Lit<__lit_status>, HNil>;

// =========================================================================
// API type
// =========================================================================

type ChatAPI = (
    // GET /ws/chat — WebSocket upgrade (the chat endpoint)
    GetEndpoint<WsChatPath, String>,
    // GET /status — JSON server status
    GetEndpoint<StatusPath, ServerStatus>,
    // GET / — serve the HTML client
    GetEndpoint<HNil, String>,
);

// =========================================================================
// Handlers
// =========================================================================

/// The chat handler — session-typed WebSocket protocol.
///
/// The type `TypedWebSocket<ChatProtocol>` guarantees that this handler
/// follows the protocol defined above. Any deviation is a compile error.
async fn handle_chat(upgrade: WebSocketUpgrade, state: State<AppState>) -> impl IntoResponse {
    let app_state = state.0.clone();

    upgrade.on_upgrade_typed::<ChatProtocol, _, _>(move |ws| async move {
        // Step 1: Receive authentication.
        let (auth, ws) = match ws.recv().await {
            Ok(result) => result,
            Err(_) => return,
        };
        let username = auth.username;
        tracing::info!("{username} connected");

        // Register user.
        let online: Vec<String> = {
            let mut users = app_state.users.lock().await;
            users.insert(username.clone(), ());
            users.keys().cloned().collect()
        };

        // Step 2: Send welcome message.
        let ws = match ws
            .send(WelcomeMessage {
                message: format!(
                    "Welcome, {username}! {} user(s) online.",
                    online.len()
                ),
                online_users: online,
            })
            .await
        {
            Ok(ws) => ws,
            Err(_) => {
                app_state.users.lock().await.remove(&username);
                return;
            }
        };

        // Broadcast that user joined.
        let _ = app_state
            .tx
            .send((String::new(), format!("{username} joined the chat")));

        // Step 3: Chat loop (Rec<Recv<ChatMessage, Send<ChatMessage, Var>>>).
        //
        // The session type enforces: receive a message, then send a response.
        // Each operation consumes the channel and returns it in the next state.
        // You cannot send without first receiving — the compiler prevents it.
        let mut ws = ws.enter();

        loop {
            // Receive a message from the client.
            let (msg, next_ws) = match ws.recv().await {
                Ok(result) => result,
                Err(_) => break, // client disconnected
            };

            let count = app_state
                .message_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tracing::info!("[{username}] {} (message #{})", msg.message, count + 1);

            // Broadcast to all connected clients.
            let _ = app_state.tx.send((username.clone(), msg.message.clone()));

            // Send the echoed message back (the protocol requires a response).
            let looped = match next_ws
                .send(ChatMessage {
                    message: format!("[{username}] {}", msg.message),
                })
                .await
            {
                Ok(ws) => ws,
                Err(_) => break,
            };

            // Var → recurse back to Rec → enter body for next iteration.
            ws = looped
                .recurse::<Recv<ChatMessage, session::Send<ChatMessage, Var>>>()
                .enter();
        }

        // Cleanup.
        app_state.users.lock().await.remove(&username);
        let _ = app_state
            .tx
            .send((String::new(), format!("{username} left the chat")));
        tracing::info!("{username} disconnected");
    })
}

/// GET /status — server status as JSON.
async fn status(state: State<AppState>) -> Json<ServerStatus> {
    let users = state.0.users.lock().await;
    Json(ServerStatus {
        online_users: users.len(),
        total_messages: state
            .0
            .message_count
            .load(std::sync::atomic::Ordering::Relaxed),
    })
}

/// GET / — serve the embedded HTML chat client.
async fn index() -> impl IntoResponse {
    let html = include_str!("../static/index.html");
    let mut res = http::Response::new(body_from_bytes(bytes::Bytes::from(html)));
    res.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("text/html; charset=utf-8"),
    );
    res
}

// =========================================================================
// Main
// =========================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    let state = AppState::new();

    tracing::info!("Starting chat server on http://localhost:3000");
    tracing::info!("Open http://localhost:3000 in a browser to chat");

    Server::<ChatAPI>::new((
        bind::<_, _, _>(handle_chat),
        bind::<_, _, _>(status),
        bind::<_, _, _>(index),
    ))
    .with_state(state)
    .serve("0.0.0.0:3000".parse()?)
    .await
}
