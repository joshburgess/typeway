# Session-Typed WebSockets

Typeway's WebSocket support uses session types to enforce correct
message ordering at compile time. If your protocol says "send a
greeting, then receive a name, then send a welcome" — the compiler
verifies you follow that exact sequence.

## Define a protocol

A protocol is a type describing the message exchange:

```rust
use typeway_core::session::*;

// Server sends a greeting, receives a name, sends a welcome, then ends.
type GreetProtocol = Send<String, Recv<String, Send<String, End>>>;
```

Session type primitives:

| Type | Meaning |
|------|---------|
| `Send<T, Next>` | Send a message of type `T`, then continue with `Next` |
| `Recv<T, Next>` | Receive a message of type `T`, then continue with `Next` |
| `End` | Close the connection |
| `Offer<L, R>` | Let the peer choose between two branches |
| `Select<L, R>` | Choose between two branches |
| `Rec<Body>` | Recursive protocol (loops) |
| `Var` | Jump to enclosing `Rec` |

## Write a handler

```rust
use typeway_server::typed_ws::TypedWebSocket;

async fn greet(ws: TypedWebSocket<GreetProtocol>) {
    // Each operation consumes `ws` and returns the channel in the next state.
    let ws = ws.send("Hello! What is your name?".to_string()).await.unwrap();
    let (name, ws) = ws.recv().await.unwrap();
    let ws = ws.send(format!("Welcome, {name}!")).await.unwrap();
    ws.close().await.unwrap();
}
```

The compiler enforces the protocol:
- Calling `.recv()` when the protocol says `Send` → compile error
- Calling `.send()` when the protocol says `Recv` → compile error
- Forgetting to `.close()` at `End` → the value is dropped, which closes

## Branching

Use `Offer` to let the client choose:

```rust
type ChatProtocol = Offer<
    Send<String, End>,    // left branch: server sends a message
    Recv<u32, End>,       // right branch: server receives a number
>;

async fn chat(ws: TypedWebSocket<ChatProtocol>) {
    match ws.offer().await.unwrap() {
        Either::Left(ws) => {
            let ws = ws.send("you chose left".to_string()).await.unwrap();
            ws.close().await.unwrap();
        }
        Either::Right(ws) => {
            let (number, ws) = ws.recv().await.unwrap();
            println!("received: {number}");
            ws.close().await.unwrap();
        }
    }
}
```

## Recursive protocols

Use `Rec` and `Var` for loops:

```rust
// Echo server: receive a message, send it back, repeat forever.
type EchoProtocol = Rec<Recv<String, Send<String, Var>>>;

async fn echo(ws: TypedWebSocket<EchoProtocol>) {
    let mut ws = ws.enter(); // enter the Rec
    loop {
        let (msg, next) = ws.recv().await.unwrap();
        let next = next.send(msg).await.unwrap();
        ws = next.recurse(); // jump back to Rec
    }
}
```

## Using WebSocketUpgrade

For REST endpoints that upgrade to WebSocket:

```rust
use typeway_server::ws::WebSocketUpgrade;

async fn ws_endpoint(upgrade: WebSocketUpgrade) -> impl IntoResponse {
    upgrade.on_upgrade_typed::<GreetProtocol, _, _>(|ws| async move {
        let ws = ws.send("hello".to_string()).await.unwrap();
        let (reply, ws) = ws.recv().await.unwrap();
        ws.close().await.unwrap();
    })
}
```

## Why session types matter

Without session types, WebSocket handlers are bags of `send` and `recv`
calls with no compile-time guarantee of correctness. Protocol violations
are runtime errors discovered in production.

With session types, the compiler verifies:
- Messages are sent and received in the correct order
- Both sides agree on the protocol (dual types)
- Branches are exhaustively handled
- The connection is properly closed

This is the same guarantee Haskell's `session-types` library provides,
but enforced naturally by Rust's ownership system instead of linear types.
