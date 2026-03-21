# Redesigning Tonic: typeway-grpc — A Type-Level, High-Performance gRPC Framework for Rust

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

## 2. Design Principles for typeway-grpc

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
                           │
typeway-grpc:       SendStream<T>, RecvStream<T>,   (the communication pattern)
                    BiStream<T, U>
                           │
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
    │
    ├─ gRPC frame header (5 bytes, parsed)
    │
    └─ Protobuf payload ──► typeway-protobuf View<'buf>
                              (zero-copy, borrows from Bytes)
                                    │
                            Middleware sees View<'buf>
                            (no allocation for logging, auth, routing)
                                    │
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
┌──────────────────────────────────────────────────────────────────┐
│                         typeway-grpc                              │
│                                                                   │
│  ┌─────────────────────┐  ┌────────────────────────────────────┐ │
│  │ Type-Level DSL       │  │ Runtime                            │ │
│  │                      │  │                                    │ │
│  │ GrpcService<Name,    │  │ ┌────────────────────────────┐    │ │
│  │   Unary<..>          │  │ │ HTTP/2 Transport            │    │ │
│  │   :+: ServerStream<> │  │ │ (hyper + Tower + rustls)    │    │ │
│  │   :+: ClientStream<> │  │ ├────────────────────────────┤    │ │
│  │   :+: BiDiStream<>   │  │ │ Session-Typed Streams       │    │ │
│  │   :+: WithMiddleware │  │ │ (SendStream, RecvStream,    │    │ │
│  │ >                    │  │ │  BiStream)                   │    │ │
│  │                      │  │ ├────────────────────────────┤    │ │
│  │ Interpreted into:    │  │ │ Typed Error Mapping         │    │ │
│  │  → Server handlers   │  │ │ (enum ↔ Status + details)  │    │ │
│  │  → Client stubs      │  │ ├────────────────────────────┤    │ │
│  │  → Reflection descs  │  │ │ Context Extractors          │    │ │
│  │  → Documentation     │  │ │ (FromContext trait)          │    │ │
│  └─────────────────────┘  │ ├────────────────────────────┤    │ │
│                            │ │ Connection Pooling           │    │ │
│                            │ │ (arena, msg reuse)           │    │ │
│                            │ └────────────────────────────┘    │ │
│                            └────────────────────────────────────┘ │
├──────────────────────────────────────────────────────────────────┤
│  typeway-protobuf                                                 │
│  (zero-copy views, typestate builders, phantom-typed fields,      │
│   RepeatedField, BytesStr, In/Out duals)                          │
├──────────────────────────────────────────────────────────────────┤
│  Typeway Framework Core                                           │
│  (type-level DSL, combinators, :+:, :*:, interpretation traits,  │
│   Tower integration, effect-system middleware)                     │
└──────────────────────────────────────────────────────────────────┘
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

## 11. Migration Strategy from Tonic

typeway-grpc maintains wire compatibility with standard gRPC, so migration is incremental:

1. **Phase 1**: Use `typeway-grpc::build()` in build.rs to generate type-level service descriptions from `.proto` files. The generated types plug into Typeway's interpretation machinery.
2. **Phase 2**: Implement handlers using `GrpcHandlers<ServiceType>`. Existing Tower middleware continues to work via Typeway's Tower integration.
3. **Phase 3**: Adopt typed error enums and context extractors.
4. **Phase 4**: Opt into zero-copy view pipeline for performance-critical paths.
5. **Phase 5**: Use session-typed streams for complex streaming protocols.
6. **Phase 6**: For mature Rust-to-Rust services, define service types directly in Rust code rather than generating from `.proto` files.

Existing gRPC clients in any language continue to work unchanged — typeway-grpc speaks standard gRPC on the wire.
