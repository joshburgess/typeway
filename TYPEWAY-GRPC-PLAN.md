# typeway-grpc: Design Vision & Implementation Plan

> This document combines the design vision and implementation plan for the typeway-grpc rewrite. For the protobuf serialization layer, see TYPEWAY-PROTOBUF-DESIGN.md.

---

# Part 1: Vision

## Executive Summary

This document proposes a redesign of Rust's gRPC story, building on the Typeway framework's Servant-inspired type-level API design philosophy. **typeway-grpc** replaces Tonic's code-generation-centric approach with type-level service descriptions that the framework interprets to derive servers, clients, and documentation — the same pattern Typeway already uses for other protocols. It builds on `typeway-protobuf` for serialization and retains full compatibility with Tower middleware, which Typeway already integrates.

---

## 1. Tonic's Current Limitations

### 1.1 Code-Generation-Centric Design

Tonic's architecture is fundamentally centered on code generation from `.proto` files. The generated code produces:

- A server trait (via `#[async_trait]`) with one method per RPC
- A client struct with methods that wrap `tonic::client::Grpc`
- All transport, framing, and encoding concerns baked into the generated output

This makes it difficult to customize behavior, swap serialization, or integrate into a type-level framework. The generated traits are opaque — you implement them, but you can't compose, inspect, or reinterpret the API description the way you can with a type-level DSL.

In contrast, Typeway defines APIs as types. Just as Haskell's Servant uses `type UserAPI = "users" :> Get '[JSON] [User]` to describe an API and then interprets that type into servers, clients, and docs, Typeway uses Rust's type system to achieve the same. typeway-grpc should follow this pattern: a gRPC service is a type, and server/client implementations are derived from that type.

### 1.2 Untyped Error Model

Tonic's `Status` type is a flat struct containing a `Code` enum, a `String` message, and opaque `Vec<u8>` details. Error details require manual protobuf decoding, and every handler returns `Result<Response<T>, Status>`, collapsing all domain errors into a single untyped bag. The `tonic-richer-error` crate was created specifically to address this gap, and its functionality was eventually merged into `tonic-types` — but it remains opt-in and runtime-checked.

### 1.3 Interceptors Are Limited

Tonic's `Interceptor` trait can only inspect/modify metadata and reject requests with a `Status`. It cannot modify request/response bodies, cannot be async, and cannot carry typed state. For anything more, you're forced into writing raw Tower `Layer`/`Service` implementations, which — while powerful — require navigating complex associated type bounds, `Pin<Box<dyn Future>>`, and generic parameter threading.

Note: **Typeway already integrates with Tower middleware and has its own effect-system-style middleware design.** typeway-grpc will build on both, not replace them. The improvements proposed here are additions to the existing Typeway middleware capabilities.

### 1.4 `Box<dyn Future>` Overhead

Tonic uses `#[async_trait]`, which desugars every async method into a `Pin<Box<dyn Future>>` — one heap allocation per RPC call. Native `async fn` in traits was stabilized in Rust 1.75, though with the caveat that `Send` bounds cannot be specified by callers on the returned future and `dyn Trait` dispatch is not supported. The `trait_variant::make` proc macro or manual `-> impl Future<Output = T> + Send` desugaring is the recommended workaround for public traits in multithreaded contexts.

> **Note:** typeway-grpc should use native `async fn` in traits where possible and fall back to `#[async_trait]` or manual desugaring only where `dyn` dispatch or explicit `Send` bounds are needed. This is tracked as a goal, not a hard requirement.

### 1.5 No Compile-Time Protocol Safety for Streams

gRPC defines four RPC patterns — unary, server-streaming, client-streaming, and bidirectional streaming. Tonic provides no compile-time guarantee that a client consuming a server-streaming RPC actually drains the stream, or that a bidirectional handler properly coordinates sends and receives. Protocol violations are runtime errors.

### 1.6 Streaming Ergonomics

Tonic streams are `Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>` — fully type-erased and heap-allocated. Creating a server-streaming response requires manually constructing a `tokio::sync::mpsc` channel, spawning a task, and wrapping the receiver.

---

## 2. Design Principles

| Principle | Technique |
|---|---|
| APIs are types, not generated code | Servant-style type-level DSL with Typeway combinators |
| Tower compatibility | Build on Typeway's existing Tower integration |
| Typed errors across the wire | Algebraic error types in service descriptions |
| Protocol adherence at compile time | Session-typed stream types at the RPC layer |
| Zero-copy message pipeline | typeway-protobuf views flow through handlers |
| Enhance existing middleware | New ideas layered onto Typeway's effect-system middleware |

---

## 3. Type-Level Service Definitions

### 3.1 The Core: gRPC Services as Types

Following Typeway's Servant-inspired philosophy, a gRPC service is described as a **type** using combinator types. The framework then interprets this type to derive server implementations, client stubs, reflection metadata, and documentation:

```rust
use typeway::api::*;
use typeway_grpc::*;
use typeway_protobuf::*;

// The service is a TYPE, not a trait, not generated code.
// Typeway interprets this type into servers, clients, docs, etc.
type UserService = GrpcService<"users.v1.UserService",
    // Each RPC is a type-level description
    Unary<"GetUser",    GetUserRequest,    User,    GetUserError>
    :+: Unary<"CreateUser", CreateUserRequest, User,    CreateUserError>
    :+: ServerStream<"ListUsers", ListUsersRequest, User, ListUsersError>
    :+: ClientStream<"UploadPhoto", PhotoChunk, UploadResult, UploadError>
    :+: BiDiStream<"Chat", ChatMessage, ChatMessage, ChatError>
>;
```

Here:
- `GrpcService<Name, Endpoints>` is a type-level combinator that marks a gRPC service
- `Unary<Name, Req, Resp, Err>` describes a unary RPC with typed request, response, and error
- `ServerStream`, `ClientStream`, `BiDiStream` describe streaming patterns
- `:+:` is the type-level alternative combinator (analogous to Servant's `:<|>`)
- The string literals are type-level `&'static str` constants (Rust's const generics)

The framework interprets this type to produce:
- A **server handler type** (what the implementor provides)
- A **client struct** (with typed methods for each RPC)
- A **gRPC reflection descriptor** (for runtime service discovery)
- A **wire codec** (using typeway-protobuf)

### 3.2 Server Implementation via Type Interpretation

The Typeway framework derives the expected handler type from the service description. The implementor provides a value that matches this derived type:

```rust
// Typeway interprets UserService into a handler type:
// ServerHandler<UserService> =
//     (Context, GetUserRequest) -> Future<Result<User, GetUserError>>
//     :*: (Context, CreateUserRequest) -> Future<Result<User, CreateUserError>>
//     :*: (Context, ListUsersRequest) -> Future<Result<SendStream<User>, ListUsersError>>
//     :*: (Context, RecvStream<PhotoChunk>) -> Future<Result<UploadResult, UploadError>>
//     :*: (Context, BiStream<ChatMessage, ChatMessage>) -> Future<Result<(), ChatError>>

// The user provides handlers that match this structure:
struct MyUserHandlers { db: DbPool }

impl GrpcHandlers<UserService> for MyUserHandlers {
    async fn get_user(&self, ctx: Context, req: GetUserRequest)
        -> Result<User, GetUserError>
    {
        self.db.find_user(req.id).await
            .ok_or(GetUserError::NotFound { id: req.id })
    }

    async fn create_user(&self, ctx: Context, req: CreateUserRequest)
        -> Result<User, CreateUserError>
    {
        self.db.insert_user(req).await
            .map_err(CreateUserError::from)
    }

    async fn list_users(&self, ctx: Context, req: ListUsersRequest)
        -> Result<SendStream<User>, ListUsersError>
    {
        let stream = self.db.stream_users(&req).await?;
        Ok(SendStream::from_stream(stream))
    }

    async fn upload_photo(&self, ctx: Context, rx: RecvStream<PhotoChunk>)
        -> Result<UploadResult, UploadError>
    {
        let mut size = 0;
        while let Some(chunk) = rx.recv().await? {
            size += chunk.data.len();
        }
        Ok(UploadResult { size })
    }

    async fn chat(&self, ctx: Context, channel: BiStream<ChatMessage, ChatMessage>)
        -> Result<(), ChatError>
    {
        while let Some(msg) = channel.recv().await? {
            channel.send(process(msg)).await?;
        }
        Ok(())
    }
}
```

The `GrpcHandlers<UserService>` trait is derived from the type-level service description by Typeway's interpretation machinery — the same mechanism it uses for HTTP, WebSocket, or any other protocol.

### 3.3 Client Derivation from the Same Type

The same `UserService` type also derives a client:

```rust
// Typeway interprets UserService into a client type:
let client = GrpcClient::<UserService>::connect("https://api.example.com").await?;

// Every method is typed from the service description:
let user: User = client.get_user(GetUserRequest { id: 42 }).await?;

// Errors are the typed enum from the service description:
match client.get_user(req).await {
    Ok(user) => handle(user),
    Err(GetUserError::NotFound { id }) => show_404(id),
    Err(GetUserError::PermissionDenied) => redirect_login(),
    Err(GetUserError::Internal(e)) => log_and_retry(e),
}

// Streaming methods return typed stream handles:
let mut stream = client.list_users(ListUsersRequest { filter: "active" }).await?;
while let Some(user) = stream.next().await? {
    println!("{}", user.name);
}
```

### 3.4 Proto-File Compatibility

For interop with non-Rust gRPC clients/servers, typeway-grpc can also generate the type-level service description from `.proto` files at build time:

```rust
// In build.rs:
typeway_grpc::build()
    .compile_protos(&["proto/users.proto"], &["proto/"])
    // Generates: type UserService = GrpcService<"users.v1.UserService", ...>
    // Plus all message types via typeway-protobuf
    .emit()?;
```

The generated output is a set of **type aliases and type-level descriptions** — not opaque trait impls. The user can extend, compose, or reinterpret them using Typeway's standard machinery.

---

## 4. Typed Errors

### 4.1 Algebraic Error Types in the Service Description

Each RPC in the type-level description carries its error type as a parameter. typeway-grpc maps these to gRPC status codes via a derive macro:

```rust
#[derive(Debug, thiserror::Error, GrpcError)]
pub enum GetUserError {
    #[grpc(code = "NOT_FOUND")]
    #[error("user {id} not found")]
    NotFound { id: UserId },

    #[grpc(code = "PERMISSION_DENIED")]
    #[error("insufficient permissions")]
    PermissionDenied,

    #[grpc(code = "INTERNAL")]
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
```

The `#[derive(GrpcError)]` macro generates:
- `From<GetUserError> for tonic::Status` (for wire transmission)
- `TryFrom<tonic::Status> for GetUserError` (for client-side recovery)
- Structured error detail encoding in `grpc-status-details-bin` using typeway-protobuf

On the client side, the typed error enables exhaustive `match` — no more parsing opaque `Status` bytes.

---

## 5. Session-Typed Streams

### 5.1 How Streams Relate to typeway-protobuf

To be precise about the layering:

- **typeway-protobuf** handles serialization/deserialization of individual messages. It defines message types like `User`, `ChatMessage`, `PhotoChunk` — along with their View, Cow, and Owned tiers.
- **typeway-grpc** defines the RPC communication patterns. `SendStream<T>`, `RecvStream<T>`, and `BiStream<T, U>` are **RPC-layer types** that live in typeway-grpc. They carry typeway-protobuf message types as their generic parameter.

The stream types enforce *who can send and who can receive* at compile time. The protobuf types determine *what* is sent. These are orthogonal concerns:

```
typeway-protobuf:   User, ChatMessage, PhotoChunk  (what gets serialized)
                           |
typeway-grpc:       SendStream<T>, RecvStream<T>,   (the communication pattern)
                    BiStream<T, U>
                           |
Tower / HTTP/2:     framing, flow control, transport  (how bytes move)
```

### 5.2 Session-Typed Stream Enforcement

The stream types use Rust's ownership system to enforce protocol correctness:

```rust
/// Server-streaming: the server can only send, never receive.
/// Dropping this closes the stream.
pub struct SendStream<T: ProtoMessage> { /* ... */ }

impl<T: ProtoMessage> SendStream<T> {
    pub async fn send(&self, msg: T) -> Result<(), StreamError> { /* ... */ }
    // No recv method exists — the type prevents it.
}

/// Client-streaming: the server can only receive.
pub struct RecvStream<T: ProtoMessage> { /* ... */ }

impl<T: ProtoMessage> RecvStream<T> {
    pub async fn recv(&self) -> Result<Option<T>, StreamError> { /* ... */ }
    // No send method exists.
}

/// Bidirectional: both send and receive.
pub struct BiStream<Send: ProtoMessage, Recv: ProtoMessage> { /* ... */ }

impl<S: ProtoMessage, R: ProtoMessage> BiStream<S, R> {
    pub async fn send(&self, msg: S) -> Result<(), StreamError> { /* ... */ }
    pub async fn recv(&self) -> Result<Option<R>, StreamError> { /* ... */ }
}
```

The type-level service description (`ServerStream`, `ClientStream`, `BiDiStream`) determines which stream type the handler receives. A handler for a `ServerStream` RPC gets a `SendStream<T>` — attempting to call `.recv()` on it is a compile error. This is typestate applied to the stream channel.

---

## 6. Middleware Enhancements

### 6.1 Existing Foundation

Typeway already integrates with Tower middleware and has its own effect-system-style middleware design. typeway-grpc builds on both. The following are **proposed additions** to the existing middleware story, not replacements.

### 6.2 Proposed: Context Extractors for RPC Handlers

Inspired by Axum's extractor pattern, typeway-grpc could offer typed extraction from the RPC context directly in handler signatures. This would integrate with the existing middleware system by letting middleware *produce* typed values that handlers *extract*:

```rust
// Middleware produces a typed value:
async fn auth_middleware(ctx: &mut Context, next: ...) -> ... {
    let user = verify_token(ctx.metadata()).await?;
    ctx.insert(user);  // typed insertion
    next.call(ctx).await
}

// Handler extracts it — no manual ctx.get() needed:
async fn get_user(
    &self,
    user: Extract<AuthenticatedUser>,  // auto-extracted from Context
    trace: Extract<TraceId>,           // auto-extracted from metadata
    req: GetUserRequest,
) -> Result<User, GetUserError> {
    if !user.can_access(req.id) {
        return Err(GetUserError::PermissionDenied);
    }
    // ...
}
```

The `Extract<T>` mechanism would use a `FromContext` trait. If `T` wasn't inserted by any middleware in the stack, the extraction fails at startup or compile time (depending on how far the static analysis can go). This could build on Typeway's existing type-level middleware composition.

### 6.3 Proposed: Per-RPC Middleware Scoping

The type-level service description enables attaching middleware to specific RPCs rather than the entire service:

```rust
type UserService = GrpcService<"users.v1.UserService",
    // Auth middleware only on mutating RPCs
    Unary<"GetUser", GetUserRequest, User, GetUserError>
    :+: WithMiddleware<RequireAuth,
            Unary<"CreateUser", CreateUserRequest, User, CreateUserError>
        >
    :+: WithMiddleware<RequireAdmin,
            Unary<"DeleteUser", DeleteUserRequest, (), DeleteUserError>
        >
    :+: ServerStream<"ListUsers", ListUsersRequest, User, ListUsersError>
>;
```

`WithMiddleware<M, Endpoint>` is a type-level combinator that wraps an endpoint with middleware `M`. The Typeway framework interprets this by inserting the middleware into the handler chain for that specific RPC. This composes with the global middleware stack from the server builder.

### 6.4 Proposed: Typed Deadline Propagation

gRPC deadlines are a cross-cutting concern that could be modeled as an effect in Typeway's middleware system:

```rust
// A deadline-aware handler automatically gets a budget:
async fn get_user(
    &self,
    deadline: Extract<Deadline>,
    req: GetUserRequest,
) -> Result<User, GetUserError> {
    // If the deadline is already expired, this returns an error
    // before the handler even runs (enforced by middleware)
    let user = self.db.find_user(req.id)
        .with_deadline(deadline)  // propagates deadline to DB call
        .await?;
    Ok(user)
}
```

---

## 7. Performance Architecture

### 7.1 Zero-Copy Request Pipeline

typeway-grpc integrates with typeway-protobuf's tiered deserialization:

```
Incoming HTTP/2 frame (Bytes, refcounted)
    |
    +- gRPC frame header (5 bytes, parsed)
    |
    +- Protobuf payload --> typeway-protobuf View<'buf>
                              (zero-copy, borrows from Bytes)
                                    |
                            Middleware sees View<'buf>
                            (no allocation for logging, auth, routing)
                                    |
                            Handler receives View<'buf>
                            or calls .to_owned() if mutation needed
```

### 7.2 Connection-Level Message Pooling

Per-connection pools reuse allocations across requests:

```rust
struct ConnectionPool {
    decode_buf: typeway_protobuf::RepeatedField<u8>,
    encode_buf: typeway_protobuf::RepeatedField<u8>,
    message_pool: typeway_protobuf::MessagePool,
}
```

This mirrors the pooling strategy from the typeway-protobuf design (inspired by GreptimeDB's 63% deserialization speedup via `RepeatedField`).

### 7.3 Static Dispatch Middleware Stack

Because Typeway composes middleware at the type level, the resulting call chain can be monomorphized by the compiler into a single static function — no vtable lookups, no `Box<dyn Future>` at the middleware layer, with full inlining opportunity.

### 7.4 Arena-Allocated Streaming Responses

For high-throughput streaming RPCs, typeway-grpc can provide an arena allocator scoped to the RPC call, reusing memory across streamed messages rather than allocating and deallocating per message.

---

## 8. Architecture Overview

```
+------------------------------------------------------------------+
|                         typeway-grpc                              |
|                                                                   |
|  +---------------------+  +------------------------------------+ |
|  | Type-Level DSL       |  | Runtime                            | |
|  |                      |  |                                    | |
|  | GrpcService<Name,    |  | +----------------------------+    | |
|  |   Unary<..>          |  | | HTTP/2 Transport            |    | |
|  |   :+: ServerStream<> |  | | (hyper + Tower + rustls)    |    | |
|  |   :+: ClientStream<> |  | +----------------------------+    | |
|  |   :+: BiDiStream<>   |  | | Session-Typed Streams       |    | |
|  |   :+: WithMiddleware |  | | (SendStream, RecvStream,    |    | |
|  | >                    |  | |  BiStream)                   |    | |
|  |                      |  | +----------------------------+    | |
|  | Interpreted into:    |  | | Typed Error Mapping         |    | |
|  |  -> Server handlers  |  | | (enum <-> Status + details) |    | |
|  |  -> Client stubs     |  | +----------------------------+    | |
|  |  -> Reflection descs |  | | Context Extractors          |    | |
|  |  -> Documentation    |  | | (FromContext trait)          |    | |
|  +---------------------+  | +----------------------------+    | |
|                            | | Connection Pooling           |    | |
|                            | | (arena, msg reuse)           |    | |
|                            | +----------------------------+    | |
|                            +------------------------------------+ |
+------------------------------------------------------------------+
|  typeway-protobuf                                                 |
|  (zero-copy views, typestate builders, phantom-typed fields,      |
|   RepeatedField, BytesStr, In/Out duals)                          |
+------------------------------------------------------------------+
|  Typeway Framework Core                                           |
|  (type-level DSL, combinators, :+:, :*:, interpretation traits,  |
|   Tower integration, effect-system middleware)                     |
+------------------------------------------------------------------+
```

---

## 9. Summary: What Each Technique Solves

| Tonic Problem | Technique | Result |
|---|---|---|
| Code-gen-centric, opaque traits | **Type-level service DSL** (Servant-style) | Services are types: inspectable, composable, multiply-interpretable |
| Untyped `Status` error bag | **Algebraic error types** per-RPC | Exhaustive `match` on client and server |
| No protocol safety for streams | **Session-typed streams** (RPC-layer, not protobuf) | Compile-time enforcement of send/recv patterns |
| `Box<dyn Future>` per RPC | **Native `async fn` in traits** (Rust 1.75+, with caveats) | Zero-allocation dispatch where applicable |
| Manual metadata parsing | **Context extractors** (`FromContext` trait) | Typed extraction, works with existing middleware |
| Global-only middleware | **Per-RPC `WithMiddleware` combinator** | Type-level middleware scoping at the endpoint level |
| Allocation-heavy request path | **Zero-copy views** from typeway-protobuf | View<'buf> flows through middleware untouched |
| Per-message heap allocation in streams | **Arena-allocated** response pipeline | Connection-scoped message pooling |
| Tight prost coupling | **typeway-protobuf** integration | All message benefits (zero-copy, pooling, typestate) |

---

## 10. Relationship to typeway-protobuf

typeway-grpc sits above typeway-protobuf in the stack. The responsibilities are clearly separated:

| Layer | Responsibility | Key Types |
|---|---|---|
| **typeway-protobuf** | Message serialization/deserialization | `User`, `ChatMessage`, `View<'buf>`, `BytesStr` |
| **typeway-grpc** | RPC communication patterns | `SendStream<T>`, `RecvStream<T>`, `BiStream<T, U>`, `GrpcService<..>` |
| **Tower / hyper** | HTTP/2 transport, framing, flow control | `Service`, `Layer`, `Body` |

Stream types like `SendStream<User>` carry typeway-protobuf message types (`User`) as their generic parameter. The stream handles *when* messages flow; protobuf handles *how* they're encoded.

---

# Part 2: Current State & Implementation Plan

## Current State Assessment

typeway-grpc today is a proof-of-concept bridge (502 tests, 15/15 roadmap items):
- Translates gRPC requests to REST to gRPC via JSON transcoding
- Hand-written proto3 codec for binary encoding (basic, not conformance-tested)
- "Streaming" by collecting JSON arrays and splitting into frames
- All type-level machinery works (GrpcReady, ApiToProto, derive macros, etc.)

The type-level design is solid. The wire protocol implementation is not.

---

## Phase 1: Native gRPC Server (replace the bridge)

**Goal:** gRPC requests go directly to handlers without REST translation. Use hyper's native HTTP/2 support and proper trailers.

### 1.1 gRPC Codec Trait

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

### 1.2 Proper HTTP/2 Trailers

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

### 1.3 Real Streaming

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

Note: Once the native streaming infrastructure is in place, the session-typed stream types from the vision (Part 1, Section 5) — `SendStream<T>`, `RecvStream<T>`, `BiStream<T, U>` — will be built on top of this `GrpcStream` primitive, adding compile-time enforcement of send/recv directionality.

### 1.4 Direct Handler Dispatch

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

### 1.5 GrpcServes Trait

Analogous to `Serves<A>` for REST:

```rust
/// Compile-time check that a handler tuple covers every gRPC method.
pub trait GrpcServes<A: ApiSpec> {
    fn register_grpc(self, router: &mut GrpcRouter);
}
```

This verifies at compile time that every endpoint in the API has a gRPC handler registered.

### 1.6 Compression

Support gRPC compression negotiation:
- `grpc-encoding` request header — decompress incoming
- `grpc-accept-encoding` — negotiate response compression
- Algorithms: `identity` (none), `gzip`, `deflate`

Use `flate2` crate for gzip/deflate. Feature-gated behind `compression`.

### Estimated effort: ~2,000 lines replacing ~1,500 lines of current bridge code.

---

## Phase 2: Prost Integration (correct binary encoding)

**Goal:** Use prost for protobuf binary encoding/decoding. This gives us battle-tested, conformance-passing wire format handling.

### 2.1 Add prost as a real dependency

Move `prost` from optional to required (or at least strongly recommended):

```toml
[dependencies]
prost = "0.13"
prost-types = "0.13"
```

### 2.2 Build script for proto compilation

Add a `build.rs` helper that users can call to compile `.proto` files:

```rust
/// In your build.rs:
typeway_grpc::compile_protos(&["proto/service.proto"])?;

/// Or generate from the API type at build time:
typeway_grpc::compile_api_protos::<MyAPI>("MyService", "pkg.v1")?;
```

This generates prost types + tonic-style service traits from the API type, all at build time.

### 2.3 ProstCodec implementation

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

### 2.4 Dual-type bridging

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

### 2.5 Conformance testing

Run the official protobuf conformance test suite against our encoding:
- https://github.com/protocolbuffers/protobuf/tree/main/conformance
- Tests edge cases: default values, unknown fields, UTF-8 validation, NaN handling, etc.

### Estimated effort: ~800 lines of new code + build script infrastructure.

---

## Phase 3: Typeway Native Codec / typeway-protobuf Integration

**Goal:** Explore whether a typeway-specific protobuf encoder can outperform prost by leveraging compile-time knowledge about the message schema. This phase integrates with the typeway-protobuf layer described in TYPEWAY-PROTOBUF-DESIGN.md.

### Why this might work

Prost is a general-purpose protobuf codec. It encodes and decodes arbitrary messages using runtime type information (field descriptors, wire types). It's fast, but it does work at runtime that could theoretically be done at compile time.

A typeway-native codec would:
1. **Generate specialized encode/decode functions per message type at compile time.** Instead of a generic `encode_field` that dispatches on wire type at runtime, generate a function that knows the exact field layout.
2. **Avoid allocation for small messages.** Prost allocates a `Vec<u8>` for encoding. A specialized encoder could write directly to a pre-sized buffer.
3. **Skip field tag encoding for known schemas.** If both sides know the schema (typeway server + typeway client), we could use a more compact encoding.
4. **SIMD-accelerate varint encoding.** Batch-encode multiple varints using SIMD instructions.

Additionally, the typeway-protobuf zero-copy view pipeline (see Part 1, Section 7.1) enables messages to flow through middleware without allocation, which prost's owned-message model cannot match.

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

### 4.1 Native gRPC client

```rust
pub struct GrpcClient<A: ApiSpec> {
    channel: hyper::client::conn::http2::SendRequest<BoxBody>,
    codec: Box<dyn GrpcCodec<...>>,
    _api: PhantomData<A>,
}
```

Uses hyper's HTTP/2 client for proper connection management, multiplexing, and flow control.

As described in the vision (Part 1, Section 3.3), the client is derived from the same type-level service description used by the server. Every method is typed from the service description, and errors are the typed enums declared per-RPC — enabling exhaustive `match` on the client side.

### 4.2 Typed method generation

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

### 4.3 Streaming client

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

This section combines the incremental migration plan from both the implementation perspective (feature flags, API changes) and the broader adoption perspective (migrating from Tonic).

### Incremental Migration via Feature Flags

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

### Migration Path from Tonic

typeway-grpc maintains wire compatibility with standard gRPC, so migration from Tonic is also incremental:

1. **Phase 1**: Use `typeway_grpc::build()` in build.rs to generate type-level service descriptions from `.proto` files. The generated types plug into Typeway's interpretation machinery.
2. **Phase 2**: Implement handlers using `GrpcHandlers<ServiceType>`. Existing Tower middleware continues to work via Typeway's Tower integration.
3. **Phase 3**: Adopt typed error enums and context extractors.
4. **Phase 4**: Opt into zero-copy view pipeline for performance-critical paths.
5. **Phase 5**: Use session-typed streams for complex streaming protocols.
6. **Phase 6**: For mature Rust-to-Rust services, define service types directly in Rust code rather than generating from `.proto` files.

Existing gRPC clients in any language continue to work unchanged — typeway-grpc speaks standard gRPC on the wire.

---

## What We Keep

Everything in the type-level design layer is preserved:
- `ApiToProto`, `CollectRpcs`, `EndpointToRpc` — proto generation from API types
- `GrpcReady` — compile-time verification
- `#[derive(ToProtoType)]` — struct/enum to proto message
- `auto_grpc_client!` — client generation from API type
- `GrpcServiceSpec`, `generate_docs_html` — spec and docs
- `validate_proto`, `diff_protos` — tooling
- Proto parser and codegen — .proto to typeway conversion
- `GrpcWebLayer` — grpc-web support (uses trailers-in-body, which is correct for grpc-web)
- Health check, reflection — standard services
- Error details — Google's rich error model

## What We Replace

- `GrpcBridge` replaced by `GrpcRouter` (direct dispatch, no REST translation)
- `proto_codec.rs` (hand-written) replaced by `ProstCodec` or `TypewayCodec`
- `Multiplexer` gRPC path replaced by proper HTTP/2 trailer-based response handling
- Collect-and-split "streaming" replaced by real `GrpcStream` with mpsc channels

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
7. **Typed errors with exhaustive match** on both client and server
8. **Session-typed streams** enforce send/recv directionality at compile time
9. **Zero-copy message pipeline** for performance-critical paths via typeway-protobuf integration
10. **Per-RPC middleware scoping** via type-level `WithMiddleware` combinator
