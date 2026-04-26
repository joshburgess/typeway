# typeway-grpc

**gRPC as type interpretation, not code generation.**

typeway-grpc is a gRPC framework for Rust where services are described as types in the Rust type system. Instead of generating code from `.proto` files, you write a type alias. The framework interprets that type to derive servers, clients, proto files, and documentation from a single source of truth.

This is the same approach Haskell's Servant library takes for REST APIs, applied to gRPC.

```rust
type UserService = (
    GetEndpoint<UsersPath, Vec<User>>,
    GetEndpoint<UserByIdPath, User>,
    PostEndpoint<UsersPath, CreateUser, User>,
);

// One type. Multiple interpretations:
let proto  = UserService::to_proto("UserService", "users.v1");   // .proto file
let server = Server::new(app).with_grpc::<UserService>(...);     // gRPC server
let client = GrpcClient::new("http://localhost:3000", ...);      // gRPC client
let spec   = UserService::service_spec();                        // API documentation
```

The handlers you write for REST work unchanged for gRPC. No separate implementations, no generated traits, no build step.

---

## Table of Contents

- [Part 1: Design. Why This Approach](#part-1-design--why-this-approach)
  - [What Tonic Does Well (and Where It Stops)](#what-tonic-does-well-and-where-it-stops)
  - [The Core Idea: Services as Types](#the-core-idea-services-as-types)
  - [Same Handlers, Both Protocols](#same-handlers-both-protocols)
  - [Proto Files Are Derived, Not Required](#proto-files-are-derived-not-required)
  - [Typed Errors](#typed-errors)
  - [Streaming](#streaming)
  - [Middleware](#middleware)
  - [Performance: TypewayCodec](#performance-typewaycodec)
  - [Architecture Diagram](#architecture-diagram)
- [Part 2: Implementation. What Was Built](#part-2-implementation--what-was-built)
  - [The Four Phases](#the-four-phases)
  - [Benchmark Results](#benchmark-results)
  - [What's Missing](#whats-missing)
  - [Migration from Tonic](#migration-from-tonic)

---

# Part 1, Design: Why This Approach

## What Tonic Does Well (and Where It Stops)

Tonic is the standard gRPC framework for Rust. It is battle-tested, well-maintained, and has a large ecosystem. If you need production gRPC in Rust today, Tonic is the safe choice. typeway-grpc is experimental.

That said, Tonic's architecture makes certain tradeoffs that typeway-grpc attempts to address:

**Code generation is the only entry point.** Tonic generates a server trait (via `#[async_trait]`) and a client struct from `.proto` files. The generated code is opaque: you implement the trait, but you cannot compose, inspect, or reinterpret the service description. If you want to serve both REST and gRPC, you write two separate handler implementations. If you want to generate documentation, you parse the `.proto` files again with a different tool.

**Errors are untyped.** Every Tonic handler returns `Result<Response<T>, Status>`, where `Status` is a flat struct containing a code enum, a string message, and opaque `Vec<u8>` details. Domain errors collapse into this single type. The `tonic-types` crate adds structured error details, but they remain opt-in and runtime-checked. You can still return any code with any message, and the client must parse opaque bytes to recover detail.

**Interceptors are shallow.** Tonic's `Interceptor` trait can inspect metadata and reject requests, but cannot modify bodies, cannot be async, and cannot carry typed state. For anything more, you write raw Tower `Layer`/`Service` implementations with their complex associated type bounds.

**Per-RPC allocation.** `#[async_trait]` desugars every handler into `Pin<Box<dyn Future>>`, one heap allocation per RPC. Rust 1.75 stabilized native `async fn` in traits, making this avoidable in many cases.

**Streams lack protocol safety.** gRPC defines four RPC patterns (unary, server-streaming, client-streaming, bidirectional), but Tonic provides no compile-time guarantee that a server-streaming handler actually sends a stream, or that a bidirectional handler coordinates sends and receives correctly. Streams are `Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>`, fully type-erased.

None of these are bugs. They are design choices that optimize for ecosystem compatibility and `.proto`-first workflows. typeway-grpc makes different choices.

## The Core Idea: Services as Types

In typeway, an API is a Rust type:

```rust
use typeway::api::*;

type UserService = (
    GetEndpoint<UsersPath, Vec<User>>,
    GetEndpoint<UserByIdPath, User>,
    PostEndpoint<UsersPath, CreateUser, User>,
);
```

This is not a macro that generates code. It is a type alias, a description of the service in the type system. The framework provides multiple **interpreters** for this type:

| Interpreter | What it produces |
|---|---|
| `ApiToProto::to_proto(...)` | A valid `.proto` file |
| `Server::with_grpc::<Api>(...)` | A gRPC server that dispatches to handlers |
| `GrpcClient::new(...)` | A gRPC client |
| `ApiToServiceDescriptor::service_descriptor(...)` | Runtime service metadata |
| `ReflectionService::from_api::<Api>(...)` | gRPC server reflection |
| `ServiceSpec::generate::<Api>(...)` | API documentation / HTML docs page |

The key property: **adding a new endpoint to the type automatically updates all interpretations.** There is no `.proto` file to keep in sync, no generated code to regenerate, no documentation to manually update.

The `GrpcReady` trait enforces this at compile time. If you add an endpoint type whose request or response type does not implement `ToProtoType`, the compiler rejects the server construction:

```
error: `MyNewType` is not gRPC-ready: all request and response types
       must implement `ToProtoType`
```

## Same Handlers, Both Protocols

This is the property that most distinguishes typeway-grpc from other frameworks: **the same handler function serves both REST and gRPC requests.**

A handler written with standard Axum-style extractors:

```rust
async fn get_user(
    Path(id): Path<u32>,
    State(db): State<DbPool>,
) -> Result<Json<User>, AppError> {
    let user = db.find_user(id).await?;
    Ok(Json(user))
}
```

This handler serves REST requests directly. For gRPC, the native dispatcher builds synthetic request parts (path parameters, query strings, JSON body) from the decoded gRPC message so that the existing extractors work without modification. The handler never knows which protocol originated the request.

This means:

- One set of handlers to write, test, and maintain
- REST and gRPC responses are always consistent
- Adding gRPC support to an existing REST API is a configuration change, not a rewrite

The dispatch itself is O(1), gRPC method paths (e.g., `/users.v1.UserService/GetUser`) are looked up in a `HashMap` that maps directly to the handler. No REST routing is involved.

## Proto Files Are Derived, Not Required

From Rust types to `.proto`:

```rust
let proto = UserService::to_proto("UserService", "users.v1");
std::fs::write("service.proto", &proto).unwrap();
```

This generates a valid proto3 file with service definitions, message definitions, and field mappings derived from the Rust types. The generated file is compatible with `protoc`, `grpcurl`, and any standard gRPC toolchain.

From `.proto` to Rust types (for interop with existing services):

```rust
// In build.rs:
typeway_grpc::build()
    .compile_protos(&["proto/users.proto"], &["proto/"])
    .emit()?;
```

The output is a set of type aliases, not opaque generated traits. You can extend, compose, or layer middleware on them like any other typeway API type.

The proto parser handles proto3 syntax, services, RPCs, messages, `map<K, V>` fields, enums, and `import` statements. The `parse_proto_with_imports()` function resolves imports recursively from include directories with circular import detection. For complex proto files with `oneof`, the Tonic codegen bridge (`tonic-compat` feature) provides full compatibility.

## Typed Errors

Each RPC can declare its error type. The `GrpcError` derive macro maps enum variants to gRPC status codes:

```rust
#[derive(Debug, thiserror::Error, GrpcError)]
pub enum GetUserError {
    #[grpc(code = "NOT_FOUND")]
    #[error("user {id} not found")]
    NotFound { id: u32 },

    #[grpc(code = "PERMISSION_DENIED")]
    #[error("insufficient permissions")]
    PermissionDenied,

    #[grpc(code = "INTERNAL")]
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
```

The derive generates `From<GetUserError> for GrpcStatus` and `TryFrom<GrpcStatus> for GetUserError`, so the client can recover typed errors:

```rust
match client.call("GetUser", &request).await {
    Ok(user) => handle(user),
    Err(e) if e.code == GrpcCode::NotFound => show_404(),
    Err(e) => log_error(e),
}
```

The `GrpcCode` enum is defined in typeway-grpc without depending on Tonic, matching the gRPC specification's integer values directly.

Structured error details (`google.rpc.Status` with typed detail payloads) are fully integrated. The `RichGrpcStatus` type carries `Vec<ErrorDetail>` with 9 standard detail types (BadRequest, RetryInfo, DebugInfo, ErrorInfo, etc.). The gRPC client automatically parses error details from responses via `GrpcClientError::rich_details()`.

## Streaming

### Type-Level Markers

Streaming RPCs are expressed as wrapper types in the API definition:

```rust
type API = (
    GetEndpoint<UserByIdPath, User>,                            // Unary
    ServerStream<GetEndpoint<UsersPath, Vec<User>>>,            // Server-streaming
    ClientStream<PostEndpoint<UploadPath, PhotoChunk, Result>>, // Client-streaming
    BidirectionalStream<PostEndpoint<ChatPath, Msg, Msg>>,      // Bidirectional
);
```

These markers control both proto generation (`rpc Method(stream Req) returns (stream Res)`) and the runtime streaming behavior.

### Runtime Streaming

At runtime, streaming uses real `tokio::sync::mpsc` channels with backpressure (default buffer size: 32 messages):

```rust
// Server-streaming: handler gets a GrpcSender<T>
pub struct GrpcSender<T> {
    tx: tokio::sync::mpsc::Sender<Result<T, GrpcStatus>>,
}

// Client-streaming: handler gets a GrpcReceiver<T>
pub struct GrpcReceiver<T> {
    rx: tokio::sync::mpsc::Receiver<Result<T, GrpcStatus>>,
}
```

The sender/receiver types are not interchangeable. A handler for a server-streaming RPC receives a `GrpcSender`, calling `.recv()` on it is a compile error because the method does not exist. This is a lightweight form of session typing: the type system enforces which direction data flows.

## Middleware

typeway-grpc builds on Typeway's existing Tower integration and effect-system middleware. The gRPC additions include:

**gRPC-Web support.** The `web` module translates gRPC-Web requests (browser clients that cannot use raw HTTP/2) into standard gRPC requests, handling the base64 encoding and trailer-in-body format differences.

**Server reflection.** `ReflectionService::from_api::<API>("Svc", "pkg")` serves the gRPC reflection protocol, enabling `grpcurl list` and similar discovery tools.

**Health checks.** The `health` module implements the standard gRPC health checking protocol.

**Deadline propagation.** The `grpc-timeout` header is parsed and available as context for handlers.

**Per-RPC middleware scoping** is a design goal expressed in the type-level API, wrapping individual endpoints with `WithMiddleware<M, Endpoint>`, but this is aspirational architecture, not yet fully implemented.

## Performance: TypewayCodec

typeway-grpc supports three codecs, selected at construction time:

| Codec | Content-Type | Use Case |
|---|---|---|
| `JsonCodec` | `application/grpc+json` | Default. typeway-to-typeway communication. |
| `BinaryCodec` | `application/grpc+proto` | Standard gRPC client interop (grpcurl, Tonic, Postman). Uses prost-based `ProtoTranscoder`. |
| `TypewayCodecAdapter<T>` | `application/grpc+proto` | Fastest path. For message types that derive `TypewayCodec`. |

### How TypewayCodec Works

`#[derive(TypewayCodec)]` is a proc macro that generates compile-time specialized `encode` and `decode` functions for each message type:

```rust
#[derive(TypewayCodec, Serialize, Deserialize)]
struct User {
    #[proto(tag = 1)]
    id: u32,
    #[proto(tag = 2)]
    name: String,
    #[proto(tag = 3)]
    email: String,
}
```

The generated code:

- **Pre-computes buffer sizes**: `encoded_len()` calculates the exact byte count without encoding, so the output buffer is allocated once
- **No runtime field lookup**: tag numbers and wire types are compile-time constants baked into the generated function
- **No JSON intermediate**: encodes directly from Rust struct fields to protobuf binary, skipping the `serde_json::Value` step that `JsonCodec` and `BinaryCodec` use
- **Inlineable**: the generated code is a sequence of direct buffer writes that LLVM can optimize aggressively

The `TypewayCodecAdapter<T>` bridges this into the `GrpcCodec` trait system so it works with the existing dispatch infrastructure.

### Benchmark Results

Measured with Criterion, encoding Rust structs to protobuf binary:

| Message Size | TypewayCodec | Hand-Written Runtime Codec | Speedup |
|---|---|---|---|
| Small (3 fields) | 14 ns | 15 ns | 39 ns |
| Medium (8 fields) | 26 ns | 28 ns | 187 ns |
| Large (15 fields) | 69 ns | 81 ns | 309 ns |

**Decode (binary → Rust struct):**

| Message size | TypewayCodec | Prost | Hand-written |
|---|---|---|---|
| Small (3 fields) | 22 ns | 31 ns | 136 ns |
| Medium (8 fields) | 82 ns | 101 ns | 418 ns |
| Large (15 fields) | 291 ns | 362 ns | 1,064 ns |

**Summary vs. prost:** TypewayCodec is **8-15% faster on encode**, **15-30% faster on decode**, and **20-26% faster on roundtrip**. The gap widens with message complexity.

**Honest caveats:**

- These benchmarks use identical message schemas with Criterion. The prost types use `#[derive(prost::Message)]`, the same derive that prost users get in production.
- The speedup comes from compile-time field layout knowledge eliminating runtime dispatch. For workloads where serialization is not the bottleneck, this will not matter.
- Enums and `oneof` (tagged enum) fields are supported in TypewayCodec. Simple enums encode as varints; tagged enums encode as protobuf oneofs with per-variant wire types.

## Architecture Diagram

```
+------------------------------------------------------------------+
|                         typeway-grpc                              |
|                                                                   |
|  Type-Level API Description          Runtime Infrastructure       |
|  ┌─────────────────────────┐         ┌─────────────────────────┐  |
|  │ (                       │         │ GrpcMultiplexer         │  |
|  │   GetEndpoint<..>,      │────────>│   routes REST vs gRPC   │  |
|  │   PostEndpoint<..>,     │         │                         │  |
|  │   ServerStream<..>,     │         │ NativeDispatch          │  |
|  │ )                       │         │   HashMap<path, handler> │  |
|  └─────────────────────────┘         │   O(1) method lookup    │  |
|          │                           │                         │  |
|          │ interpreted into:         │ Codec Layer             │  |
|          ├── .proto file             │   JsonCodec             │  |
|          ├── server dispatch         │   BinaryCodec (prost)   │  |
|          ├── client stubs            │   TypewayCodecAdapter   │  |
|          ├── reflection metadata     │                         │  |
|          └── API documentation       │ Streaming               │  |
|                                      │   GrpcSender<T>         │  |
|                                      │   GrpcReceiver<T>       │  |
|                                      │   mpsc channels         │  |
|                                      │                         │  |
|                                      │ HTTP/2 Trailers         │  |
|                                      │   real trailers via     │  |
|                                      │   hyper TrailerBody     │  |
|                                      └─────────────────────────┘  |
+------------------------------------------------------------------+
|  Typeway Core                                                     |
|  (type-level combinators, endpoint types, path extraction,        |
|   Tower integration, effect-system middleware)                     |
+------------------------------------------------------------------+
```

---

# Part 2, Implementation: What Was Built

## The Four Phases

typeway-grpc was built incrementally over four phases. Each phase replaced a layer of indirection with direct implementation.

### Phase 1: Native gRPC Server

**Problem:** The original proof-of-concept used a "bridge" pattern: gRPC requests were translated into synthetic REST requests, routed through the REST handler, and the REST response was translated back to gRPC framing. This worked but was slow (double serialization), fragile (synthetic request construction could fail in surprising ways), and could not support real streaming.

**Solution:** `GrpcMultiplexer` dispatches gRPC requests directly to handlers via `HashMap` lookup. The `GrpcMultiplexer` sits at the HTTP layer and routes by `content-type: application/grpc*`: gRPC requests go to native dispatch, everything else goes to the REST router.

Key implementation details:
- Real HTTP/2 trailers carry `grpc-status` and `grpc-message` (via a custom `TrailerBody` type that uses hyper's trailer support)
- gRPC frame parsing handles the 5-byte length-prefixed message format
- Streaming uses `tokio::sync::mpsc` channels with backpressure, not collect-and-split

The bridge (`GrpcBridge`, the old `Multiplexer`) was removed entirely. The `grpc-native` feature flag was folded into the `grpc` feature.

### Phase 2: Binary Protobuf (Prost Integration)

**Problem:** Phase 1 used JSON encoding (`application/grpc+json`), which works for typeway-to-typeway communication but is not compatible with standard gRPC clients like `grpcurl`, Tonic-based services, or Postman.

**Solution:** `BinaryCodec` provides standard `application/grpc+proto` encoding via prost-based `ProtoTranscoder`. The codec trait (`GrpcCodec`) abstracts over encoding format, so the dispatch layer is codec-agnostic.

The `ProtoTranscoder` maps between `serde_json::Value` (the internal representation handlers work with) and protobuf binary, using runtime field descriptors generated from the API types.

### Phase 3: TypewayCodec

**Problem:** `BinaryCodec` goes through a JSON intermediate: Rust struct -> `serde_json::Value` -> protobuf binary. This double conversion is unnecessary when both endpoints understand the message types.

**Solution:** `#[derive(TypewayCodec)]` generates compile-time specialized encode/decode functions that work directly on struct fields. The `TypewayCodecAdapter<T>` wraps these as a `GrpcCodec` implementation.

The codec generates two traits:
- `TypewayEncode`, `encoded_len()` + `encode_to(&self, buf: &mut Vec<u8>)`
- `TypewayDecode`, `typeway_decode(bytes: &[u8]) -> Result<Self, TypewayDecodeError>`

Error handling is thorough: `TypewayDecodeError` covers unexpected EOF, varint overflow, unknown wire types, invalid field values, and unknown enum discriminants.

### Phase 4: Client Rewrite

**Problem:** The original `grpc_client!` macro generated JSON-only clients. With multiple codecs available, the client needed to select encoding automatically.

**Solution:** `GrpcClient` is codec-aware. It carries an `Arc<dyn GrpcCodec>` and supports:
- Unary calls (`call`)
- Server-streaming calls (`call_server_stream`)
- Request interceptors (applied in order before sending)
- Default metadata (headers sent with every request)
- Configurable timeouts

The client uses `reqwest` as its HTTP transport.

## Full Feature List

What ships today in typeway-grpc:

- **Proto generation**: `ApiToProto::to_proto()` from Rust types
- **Proto parsing**: `ProtoFile::parse()` for proto3 files (services, messages, map fields)
- **Proto diffing**: detect additions, removals, and type changes between proto versions
- **Proto validation**: check for breaking changes
- **Native gRPC dispatch**: `HashMap`-based O(1) routing to handlers
- **Three codecs**: JSON, binary protobuf (prost), TypewayCodec
- **Real HTTP/2 trailers**: proper `grpc-status` via `TrailerBody`
- **Streaming**: server, client, and bidirectional via mpsc channels
- **Server reflection**: `grpc.reflection.v1alpha` protocol
- **Health checks**: standard gRPC health checking
- **gRPC-Web**: browser client compatibility
- **Compression**: gzip support (behind feature flag)
- **Tonic codegen bridge**: `tonic-compat` feature for existing `.proto` workflows
- **Service specs**: runtime service metadata and HTML documentation
- **Compile-time readiness**: `GrpcReady` trait ensures all types are proto-compatible
- **gRPC client**: codec-aware with interceptors and streaming
- **350+ tests, 0 warnings, 0 clippy lints**

## Benchmark Results

TypewayCodec vs. prost vs. hand-written runtime codec, measured with Criterion:

```
Encode (Rust struct → protobuf binary):
  small:   TypewayCodec 14ns | Prost 15ns | Hand-written 39ns
  medium:  TypewayCodec 26ns | Prost 28ns | Hand-written 187ns
  large:   TypewayCodec 69ns | Prost 81ns | Hand-written 309ns

Decode (protobuf binary → Rust struct):
  small:   TypewayCodec 22ns | Prost 31ns | Hand-written 136ns
  medium:  TypewayCodec 82ns | Prost 101ns | Hand-written 418ns
  large:   TypewayCodec 291ns | Prost 362ns | Hand-written 1064ns
```

The gains over prost come from compile-time field layout knowledge: tag numbers, wire types, and buffer sizes are constants, not runtime values. The gap widens with message complexity because each additional field is one more branch prost evaluates at runtime that TypewayCodec resolves at compile time.

## What's Shipped vs. What's Missing

### Shipped

- **Structured error details.** `RichGrpcStatus` with 9 standard `google.rpc.Status` detail types. Client auto-parses error details from responses.
- **Enum + oneof support.** `#[derive(TypewayCodec)]` handles simple enums (varint) and tagged enums (oneof). Proto parser handles `enum` blocks.
- **Import resolution.** `parse_proto_with_imports()` resolves imports recursively from include directories.
- **Connection pooling.** `GrpcClientPool` shares HTTP/2 connections across multiple `GrpcClient` instances with configurable pool size and timeouts.
- **Retry + circuit breaker.** `GrpcRetryPolicy` with exponential backoff + jitter. `CircuitBreaker` with Closed→Open→HalfOpen state machine. Both integrated into `GrpcClient::send_request()`.
- **Per-RPC middleware.** `GrpcRouter::add_middleware()` registers per-method middleware that runs between request decode and handler call.
- **Arena pooling.** `BufPool` provides thread-safe reusable encode buffers for zero-allocation steady-state encoding.
- **GAT-based zero-copy views.** `MessageView` trait with `type View<'buf>` for borrowed decode without allocation.
- **Typestate builders.** `#[derive(TypestateBuilder)]` with `#[required]` fields, `.build()` only compiles when all required fields are set.
- **gRPC conformance testing.** Smoke + interop test suite covering proto validation, framing, status codes, error details, proto diffing, retry defaults, and circuit breaker state transitions.
- **Proto-first codegen.** `.proto` → Rust types with TypewayCodec + ToProtoType + BytesStr, via library API or CLI (`typeway-grpc api-from-proto --codec`).
- **OpenAPI bidirectional codegen.** Swagger 2.x ↔ Rust, OpenAPI 3.x ↔ Rust, parse specs to generate typeway types, or generate specs from types.
- **ServerBuilder with `.mount()`.** Compose large APIs from sub-APIs with type-level mount + effect tracking. `.build()` checks both `AllMounted` and `AllProvided`.

### Remaining gaps

- **Production use.** typeway-grpc has not been deployed in production. Tonic has. This matters.
- **Official gRPC conformance suite.** Smoke tests are in place but the official gRPC interop test suite has not been run.
- **Load balancing.** Retry and circuit breaking exist, but there is no built-in load balancer or service discovery.

## Migration from Tonic

For projects currently using Tonic, typeway-grpc offers two migration paths:

**Incremental (recommended):** Use the `tonic-compat` feature to keep existing `.proto`-generated code while gradually defining services as types. The Tonic codegen bridge lets you use Tonic's generated types with typeway's dispatch infrastructure.

**Full migration:** Replace `.proto` files with type-level API definitions. Use `ApiToProto` to generate `.proto` files for non-Rust clients that still need them. Rewrite handlers to use Axum-style extractors.

The incremental path is lower risk. The full migration path gives you the dual-protocol handlers and single-source-of-truth benefits.

---

## Summary

| Tonic Approach | typeway-grpc Approach |
|---|---|
| `.proto` files are the source of truth | Rust types are the source of truth |
| Code generation produces server traits | Type interpretation derives servers |
| Separate REST and gRPC handlers | Same handlers serve both protocols |
| `Status` with opaque details | Typed error enums with derive macros |
| `Pin<Box<dyn Stream>>` | Typed `GrpcSender<T>` / `GrpcReceiver<T>` |
| `#[async_trait]` (heap alloc per RPC) | Native async where possible |
| Runtime codec selection | Compile-time specialized codec (15-30% faster than prost on decode) |
| Proto files are input | Proto files are output (or input, your choice) |

typeway-grpc is experimental. Tonic is not. Choose accordingly, but if the idea of services-as-types appeals to you, this is what that looks like for gRPC in Rust.
