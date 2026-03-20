# typeway-grpc Rewrite Plan: Production-Grade gRPC from First Principles

## Goal

Replace the current JSON-bridge gRPC implementation with a production-grade gRPC server that matches tonic's protocol correctness and performance while keeping typeway's type-first API design. Eventually, explore whether we can beat prost's encoding performance with a typeway-native protobuf codec.

---

## Current State

typeway-grpc today is a proof-of-concept bridge (502 tests, 15/15 roadmap items):
- Translates gRPC requests → REST → gRPC via JSON transcoding
- Hand-written proto3 codec for binary encoding (basic, not conformance-tested)
- "Streaming" by collecting JSON arrays and splitting into frames
- All type-level machinery works (GrpcReady, ApiToProto, derive macros, etc.)

The type-level design is solid. The wire protocol implementation is not.

---

## Phase 1: Native gRPC Server (replace the bridge)

**Goal:** gRPC requests go directly to handlers without REST translation. Use hyper's native HTTP/2 support and proper trailers.

### 1.1 — gRPC Codec Trait

Replace the current bridge's JSON-in/JSON-out pattern with a proper codec abstraction:

```rust
/// Encodes and decodes gRPC messages.
pub trait GrpcCodec<T> {
    fn encode(&self, msg: &T) -> Result<Bytes, CodecError>;
    fn decode(&self, bytes: &[u8]) -> Result<T, CodecError>;
    fn content_type(&self) -> &'static str;
}

/// JSON codec (existing behavior, kept for grpc-web and testing)
pub struct JsonCodec;

/// Prost binary codec (production, uses prost for encode/decode)
pub struct ProstCodec;

/// Typeway native codec (future Phase 3 — our own protobuf encoder)
pub struct TypewayCodec;
```

The server is generic over the codec, defaulting to `ProstCodec` when available.

### 1.2 — Proper HTTP/2 Trailers

Replace trailers-in-body with real HTTP/2 trailers. Hyper supports this:

```rust
// Current (wrong for standard gRPC):
parts.headers.insert("grpc-status", ...);

// Correct:
let mut trailers = http::HeaderMap::new();
trailers.insert("grpc-status", status_value);
trailers.insert("grpc-message", message_value);
// Send trailers via hyper's trailer support
```

This requires changing how we build the response body. Instead of a single collected body with appended trailer bytes, we use hyper's `Body` with a trailers future.

Key files to change:
- `typeway-server/src/grpc.rs` — Multiplexer response construction
- `typeway-grpc/src/framing.rs` — remove trailers-in-body encoding for the native path (keep for grpc-web)

### 1.3 — Real Streaming

Replace collect-and-split with proper async streaming:

```rust
/// A stream of gRPC messages.
pub struct GrpcStream<T> {
    inner: tokio::sync::mpsc::Receiver<Result<T, GrpcStatus>>,
}

/// Server-side: handler returns a GrpcStream
async fn list_users(state: State<Db>) -> GrpcStream<User> {
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move {
        for user in db.all_users().await {
            if tx.send(Ok(user)).await.is_err() { break; }
        }
    });
    GrpcStream { inner: rx }
}
```

The gRPC server reads from the stream and sends each message as a length-prefixed frame over the HTTP/2 connection. Backpressure is handled by the mpsc channel — when the client can't keep up, the channel fills and the producer blocks.

For the bridge (backward compat), keep the JSON-array-splitting behavior. The native path uses real streams.

### 1.4 — Direct Handler Dispatch

The current bridge rewrites gRPC requests as REST requests and forwards to the router. The native path dispatches directly:

```rust
/// A gRPC handler that processes a request and produces a response.
pub trait GrpcHandler<E> {
    fn call(
        &self,
        request: GrpcRequest<E::Req>,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<E::Res>, GrpcStatus>> + Send>>;
}
```

The `GrpcHandler` trait is implemented for the same async functions that serve REST, via the extractor pattern. A handler like:

```rust
async fn get_user(path: Path<UserByIdPath>, state: State<Db>) -> Json<User>
```

Works for both REST (extracts from HTTP request parts) and gRPC (extracts from the proto message fields). The extraction source differs, but the handler signature is the same.

This requires a `GrpcFromRequest` trait that extracts handler arguments from a decoded protobuf message:

```rust
pub trait GrpcFromRequest<T: prost::Message>: Sized {
    fn from_grpc_request(msg: &T) -> Result<Self, GrpcStatus>;
}
```

### 1.5 — GrpcServes Trait

Analogous to `Serves<A>` for REST:

```rust
/// Compile-time check that a handler tuple covers every gRPC method.
pub trait GrpcServes<A: ApiSpec> {
    fn register_grpc(self, router: &mut GrpcRouter);
}
```

This verifies at compile time that every endpoint in the API has a gRPC handler registered.

### 1.6 — Compression

Support gRPC compression negotiation:
- `grpc-encoding` request header → decompress incoming
- `grpc-accept-encoding` → negotiate response compression
- Algorithms: `identity` (none), `gzip`, `deflate`

Use `flate2` crate for gzip/deflate. Feature-gated behind `compression`.

### Estimated effort: ~2,000 lines replacing ~1,500 lines of current bridge code.

---

## Phase 2: Prost Integration (correct binary encoding)

**Goal:** Use prost for protobuf binary encoding/decoding. This gives us battle-tested, conformance-passing wire format handling.

### 2.1 — Add prost as a real dependency

Move `prost` from optional to required (or at least strongly recommended):

```toml
[dependencies]
prost = "0.13"
prost-types = "0.13"
```

### 2.2 — Build script for proto compilation

Add a `build.rs` helper that users can call to compile `.proto` files:

```rust
/// In your build.rs:
typeway_grpc::compile_protos(&["proto/service.proto"])?;

/// Or generate from the API type at build time:
typeway_grpc::compile_api_protos::<MyAPI>("MyService", "pkg.v1")?;
```

This generates prost types + tonic-style service traits from the API type, all at build time.

### 2.3 — ProstCodec implementation

```rust
impl<T: prost::Message + Default> GrpcCodec<T> for ProstCodec {
    fn encode(&self, msg: &T) -> Result<Bytes, CodecError> {
        Ok(Bytes::from(msg.encode_to_vec()))
    }
    fn decode(&self, bytes: &[u8]) -> Result<T, CodecError> {
        T::decode(bytes).map_err(|e| CodecError::Decode(e.to_string()))
    }
    fn content_type(&self) -> &'static str {
        "application/grpc"
    }
}
```

### 2.4 — Dual-type bridging

For types that have both `serde` and `prost::Message` impls:

```rust
#[derive(Debug, Serialize, Deserialize, prost::Message, ToProtoType)]
pub struct User {
    #[prost(uint32, tag = "1")]
    #[proto(tag = 1)]
    pub id: u32,
    #[prost(string, tag = "2")]
    #[proto(tag = 2)]
    pub name: String,
}
```

The `#[derive(ToProtoType)]` macro could optionally generate `prost::Message` derive attributes. Or we provide a `#[derive(TypewayProto)]` that generates both `ToProtoType` AND `prost::Message`.

### 2.5 — Conformance testing

Run the official protobuf conformance test suite against our encoding:
- https://github.com/protocolbuffers/protobuf/tree/main/conformance
- Tests edge cases: default values, unknown fields, UTF-8 validation, NaN handling, etc.

### Estimated effort: ~800 lines of new code + build script infrastructure.

---

## Phase 3: Typeway Native Codec (beat prost)

**Goal:** Explore whether a typeway-specific protobuf encoder can outperform prost by leveraging compile-time knowledge about the message schema.

### Why this might work

Prost is a general-purpose protobuf codec. It encodes and decodes arbitrary messages using runtime type information (field descriptors, wire types). It's fast, but it does work at runtime that could theoretically be done at compile time.

A typeway-native codec would:
1. **Generate specialized encode/decode functions per message type at compile time.** Instead of a generic `encode_field` that dispatches on wire type at runtime, generate a function that knows the exact field layout.
2. **Avoid allocation for small messages.** Prost allocates a `Vec<u8>` for encoding. A specialized encoder could write directly to a pre-sized buffer.
3. **Skip field tag encoding for known schemas.** If both sides know the schema (typeway server + typeway client), we could use a more compact encoding.
4. **SIMD-accelerate varint encoding.** Batch-encode multiple varints using SIMD instructions.

### Why this might NOT work

1. Prost is already highly optimized. The low-hanging fruit (avoid copies, reuse buffers) is already picked.
2. The protobuf wire format IS the bottleneck — you can't change it and stay compatible.
3. Compile-time specialization benefits are small when the hot loop is already tight.
4. Maintaining a custom codec is a permanent burden.

### Approach

1. **Benchmark prost first.** Profile encode/decode for typical message sizes (small: 50B, medium: 1KB, large: 100KB). Identify where time is spent.
2. **Generate specialized encoders via proc-macro.** `#[derive(TypewayCodec)]` generates a hand-unrolled encode function:
   ```rust
   // Generated by the macro — no runtime dispatch:
   fn encode_user(user: &User, buf: &mut Vec<u8>) {
       // Field 1: uint32 id
       buf.push(0x08); // tag 1, wire type 0
       encode_varint(buf, user.id as u64);
       // Field 2: string name
       buf.push(0x12); // tag 2, wire type 2
       encode_varint(buf, user.name.len() as u64);
       buf.extend_from_slice(user.name.as_bytes());
   }
   ```
3. **Benchmark against prost.** If we're faster, ship it. If not, use prost and move on.
4. **Criterion benchmarks** comparing: typeway codec, prost, and the current hand-written codec. Test with the realworld app's domain types.

### Decision gate

After Step 3, if the typeway codec is:
- **>20% faster than prost:** Ship it as the default, keep prost as a fallback.
- **Within 20% of prost:** Use prost. The maintenance burden of a custom codec isn't worth a marginal speedup.
- **Slower than prost:** Use prost. Delete the native codec.

### Estimated effort: ~1,500 lines for the proc-macro + benchmarks. May result in 0 shipped lines if prost wins.

---

## Phase 4: Client Rewrite

**Goal:** Replace the `grpc_client!` and `auto_grpc_client!` JSON-based clients with proper gRPC clients using the chosen codec.

### 4.1 — Native gRPC client

```rust
pub struct GrpcClient<A: ApiSpec> {
    channel: hyper::client::conn::http2::SendRequest<BoxBody>,
    codec: Box<dyn GrpcCodec<...>>,
    _api: PhantomData<A>,
}
```

Uses hyper's HTTP/2 client for proper connection management, multiplexing, and flow control.

### 4.2 — Typed method generation

The `auto_grpc_client!` macro generates typed methods that use the native codec:

```rust
auto_grpc_client! {
    pub struct UserClient;
    api = UserAPI;
    service = "UserService";
    package = "users.v1";
}

// Generated:
impl UserClient {
    pub async fn list_users(&self) -> Result<Vec<User>, GrpcStatus> { ... }
    pub async fn get_user(&self, id: u32) -> Result<User, GrpcStatus> { ... }
    pub async fn list_users_stream(&self) -> Result<GrpcStream<User>, GrpcStatus> { ... }
}
```

### 4.3 — Streaming client

```rust
/// Send a stream of messages to the server.
pub async fn create_users_stream(
    &self,
    stream: impl futures::Stream<Item = CreateUserRequest>,
) -> Result<BatchResponse, GrpcStatus> { ... }
```

### Estimated effort: ~1,000 lines.

---

## Migration Strategy

The rewrite happens incrementally behind feature flags:

1. **`grpc-bridge` (current default):** The existing JSON bridge. No breaking changes.
2. **`grpc-native`:** The new native gRPC server with proper trailers, streaming, and codec.
3. **`grpc-prost`:** Prost-based binary encoding (requires prost dependency).
4. **`grpc-typeway-codec`:** The experimental native codec (Phase 3, if it beats prost).

Users migrate by changing their feature flag. The API type and handler signatures stay the same — only the wire protocol changes.

Old:
```rust
// Bridge mode (JSON transcoding)
Server::<API>::new(handlers).with_grpc("Svc", "pkg").serve(addr).await?;
```

New:
```rust
// Native mode (proper gRPC)
Server::<API>::new(handlers).with_grpc_native("Svc", "pkg").serve(addr).await?;
```

---

## What We Keep

Everything in the type-level design layer is preserved:
- `ApiToProto`, `CollectRpcs`, `EndpointToRpc` — proto generation from API types
- `GrpcReady` — compile-time verification
- `#[derive(ToProtoType)]` — struct/enum → proto message
- `auto_grpc_client!` — client generation from API type
- `GrpcServiceSpec`, `generate_docs_html` — spec and docs
- `validate_proto`, `diff_protos` — tooling
- Proto parser and codegen — .proto ↔ typeway conversion
- `GrpcWebLayer` — grpc-web support (uses trailers-in-body, which is correct for grpc-web)
- Health check, reflection — standard services
- Error details — Google's rich error model

## What We Replace

- `GrpcBridge` → `GrpcRouter` (direct dispatch, no REST translation)
- `proto_codec.rs` (hand-written) → `ProstCodec` or `TypewayCodec`
- `Multiplexer` gRPC path → proper HTTP/2 trailer-based response handling
- Collect-and-split "streaming" → real `GrpcStream` with mpsc channels

---

## Estimated Total Effort

| Phase | Lines | Depends on |
|---|---|---|
| Phase 1: Native server | ~2,000 | Nothing — can start now |
| Phase 2: Prost integration | ~800 | Phase 1 |
| Phase 3: Native codec | ~1,500 (may be discarded) | Phase 2 (for benchmarking) |
| Phase 4: Client rewrite | ~1,000 | Phase 1 |
| **Total** | **~5,300** | |

Phases 1 and 2 are the critical path. Phase 3 is exploratory. Phase 4 follows naturally from Phase 1.

---

## Success Criteria

1. **Passes gRPC conformance tests** (or at least the proto3 subset)
2. **Standard gRPC clients work out of the box** (grpcurl, Postman, tonic clients)
3. **Real streaming with backpressure** (not collect-and-split)
4. **Performance within 10% of tonic** for unary RPCs
5. **The type-first API is preserved** — same `Server::<API>::new(handlers).with_grpc()` pattern
6. **Incremental migration** — old bridge mode still works behind a feature flag
