# Session-Typed Chat Server

A chat server where the WebSocket protocol is enforced **at compile time**.

The type system guarantees that:
- The server sends a welcome before accepting messages
- The client must authenticate before chatting
- Messages alternate: receive then send, receive then send
- If a handler sends messages in the wrong order, **it won't compile**

## The Protocol

```rust
// Defined as a type — not documentation, not comments. A TYPE.
type ChatProtocol = Recv<AuthRequest, Send<WelcomeMessage, ChatLoop>>;
type ChatLoop = Rec<Recv<ChatMessage, Send<ChatMessage, Var>>>;
```

This says:
1. **Receive** an `AuthRequest` (username)
2. **Send** a `WelcomeMessage` (greeting + online users)
3. **Loop**: receive a `ChatMessage`, send a `ChatMessage`, repeat

Each step is enforced by the compiler. Try reordering them — it won't build.

## Run

```bash
cargo run -p typeway-chat
```

Open [http://localhost:3000](http://localhost:3000) in a browser.

## What makes this special

No other Rust framework does this. Axum, Actix, and Warp all give you
raw `WebSocket` handles with `send()` and `recv()` — you can call them
in any order, and protocol violations are runtime errors discovered in
production.

Typeway's `TypedWebSocket<S>` consumes itself on each operation and
returns the channel in the next protocol state. Rust's ownership system
enforces the session type. Haskell has session type libraries, but they
require linear types. Rust gets linearity for free via move semantics.

## Architecture

```
Client                    Server
  |                         |
  |--- {"username":"..."}-->|  Recv<AuthRequest, ...>
  |<-- {"message":"..."}  --|  Send<WelcomeMessage, ...>
  |                         |
  |--- {"message":"..."}-->|  Rec<Recv<ChatMessage, ...>>
  |<-- {"message":"..."}  --|       Send<ChatMessage, ...>
  |--- {"message":"..."}-->|       Var → recurse
  |<-- {"message":"..."}  --|
  |         ...             |
```

## Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Server with session-typed WebSocket handler |
| `static/index.html` | Browser chat client |

## API

| Endpoint | Description |
|----------|-------------|
| `GET /` | HTML chat client |
| `GET /ws/chat` | WebSocket chat endpoint |
| `GET /status` | Server status (JSON) |
