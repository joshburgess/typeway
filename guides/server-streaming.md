# Server Streaming with GrpcStream

Server streaming lets you send multiple messages to the client over a
single RPC call. Use it for feeds, large result sets, or any scenario
where you produce items incrementally.

## Basic pattern

```rust
use typeway::prelude::*;
use typeway_server::GrpcStream;

async fn list_events(state: State<Db>) -> GrpcStream<Event> {
    let (tx, stream) = GrpcStream::channel(32); // buffer size = 32

    // Spawn a task that sends items into the stream.
    let db = state.0.clone();
    tokio::spawn(async move {
        let mut cursor = db.events_cursor().await;
        while let Some(event) = cursor.next().await {
            if tx.send(event).await.is_err() {
                break; // client disconnected
            }
        }
        // tx is dropped here → stream closes → client gets trailers
    });

    stream
}
```

The handler returns a `GrpcStream<T>` immediately. The spawned task
sends items through the channel. When the sender is dropped, the
stream closes and the client receives gRPC trailers with `grpc-status: 0`.

## Backpressure

The channel buffer controls backpressure. When the buffer is full,
`tx.send()` waits until the client reads a message:

```rust
// Small buffer: tight backpressure, low memory
let (tx, stream) = GrpcStream::channel(4);

// Large buffer: bursty producers can run ahead
let (tx, stream) = GrpcStream::channel(1024);
```

For database cursors, a buffer of 32-64 is typical. For real-time
feeds, 4-8 keeps latency low.

## Error handling

Send an error status to terminate the stream early:

```rust
use typeway_grpc::status::{GrpcCode, GrpcStatus};

tokio::spawn(async move {
    for item in items {
        match validate(&item) {
            Ok(valid) => {
                if tx.send(valid).await.is_err() { break; }
            }
            Err(e) => {
                tx.send_error(GrpcStatus {
                    code: GrpcCode::InvalidArgument,
                    message: format!("invalid item: {e}"),
                }).await.ok();
                return; // stop streaming
            }
        }
    }
});
```

The client receives the error as the final gRPC status in trailers.

## API type setup

Mark the endpoint as server-streaming in your API type:

```rust
use typeway_grpc::streaming::ServerStream;

type API = (
    // Normal unary endpoint
    PostEndpoint<UsersPath, CreateUser, User>,
    // Server-streaming endpoint
    ServerStream<GetEndpoint<EventsPath, Vec<Event>>>,
);
```

The `ServerStream<E>` wrapper:
- Generates `returns (stream Event)` in the `.proto` output
- At the REST layer, the handler returns `Vec<T>` as a JSON array
- At the gRPC layer, each item is sent as a separate gRPC frame

## REST vs gRPC behavior

The same handler can serve both REST and gRPC:

- **REST**: The handler returns `GrpcStream<T>`, which the framework
  collects into a JSON array (`[item1, item2, ...]`).
- **gRPC**: Each item is sent as a separate length-prefixed frame.
  The client receives them one at a time via its streaming iterator.

For REST-only handlers that return `Json<Vec<T>>`, the gRPC layer
splits the JSON array into individual frames automatically. You don't
need `GrpcStream<T>` unless you want true frame-by-frame streaming
with backpressure.
