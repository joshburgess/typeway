# gRPC / Tonic Interop: Design & Implementation Plan

## Overview

This document covers two things: what works today with zero new code (items 1-3) and a detailed implementation plan for the novel feature — generating gRPC service definitions from typeway's API type (item 4).

---

## Part 1: What Works Today

These three patterns require no new code — just documentation and examples.

### 1. Shared Tower Middleware

Both typeway and Tonic implement `tower::Service`. Any Tower layer works with both:

```rust
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tower_http::cors::CorsLayer;

// Same middleware stack for both
let middleware = ServiceBuilder::new()
    .layer(TraceLayer::new_for_http())
    .layer(CorsLayer::permissive());

// Apply to typeway
let rest = middleware.clone().service(typeway_server.into_service());

// Apply to Tonic
let grpc = middleware.service(tonic_server.into_service());
```

Custom auth middleware, rate limiting, request ID injection — all shared. Write once, apply to both.

### 2. Side-by-Side Serving on One Port

Use `hyper` directly to multiplex based on the `content-type` header. gRPC uses `application/grpc`; everything else goes to typeway:

```rust
use hyper::service::service_fn;

let rest_svc = typeway_server.into_service();
let grpc_svc = tonic_server.into_service();

let make_svc = service_fn(move |req: Request<Incoming>| {
    let rest = rest_svc.clone();
    let grpc = grpc_svc.clone();
    async move {
        if req.headers().get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.starts_with("application/grpc"))
        {
            grpc.call(req).await
        } else {
            rest.call(req).await
        }
    }
});
```

This gives you REST and gRPC on the same port, same binary, same runtime. Tonic also provides `tonic::transport::server::Routes` for this — the approach above is more explicit.

### 3. Shared Types with Dual Serialization

Derive both `serde` and `prost::Message` on the same Rust types:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, prost::Message)]
pub struct User {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(string, tag = "3")]
    pub email: String,
}
```

This `User` struct works in typeway handlers (`Json<User>`) and in Tonic handlers (`tonic::Request<User>`). One struct, two protocols.

**Limitation:** `prost::Message` derive requires `#[prost(...)]` attributes on every field, which is verbose. An alternative is defining the types in a `.proto` file and using `prost-build` to generate the Rust types, then implementing `serde::Serialize`/`Deserialize` via a wrapper or `prost-serde`.

---

## Part 2: API Type → .proto Generation (`typeway-grpc`)

This is the novel feature: given a typeway API type, generate a `.proto` file and a Tonic service implementation.

### Architecture

```
typeway-grpc/
    Cargo.toml
    src/
        lib.rs           — public API
        proto_gen.rs     — API type → .proto file generation
        mapping.rs       — Rust type → protobuf type mapping
        service_gen.rs   — API type → Tonic service trait generation
```

**Workspace member:** `typeway-grpc` in the workspace, optional dependency.

**Feature flag:** `grpc` on the facade crate.

### How It Works

#### Step 1: Type-to-Proto Mapping

Define a trait `ToProtoType` that maps Rust types to protobuf type names:

```rust
pub trait ToProtoType {
    /// The protobuf type string (e.g., "string", "uint32", "User").
    fn proto_type() -> &'static str;

    /// Whether this is a message type (needs its own message definition).
    fn is_message() -> bool { false }

    /// Generate the message definition, if this is a message type.
    fn message_def() -> Option<String> { None }
}

// Primitive mappings
impl ToProtoType for String { fn proto_type() -> &'static str { "string" } }
impl ToProtoType for u32 { fn proto_type() -> &'static str { "uint32" } }
impl ToProtoType for u64 { fn proto_type() -> &'static str { "uint64" } }
impl ToProtoType for i32 { fn proto_type() -> &'static str { "int32" } }
impl ToProtoType for i64 { fn proto_type() -> &'static str { "int64" } }
impl ToProtoType for f32 { fn proto_type() -> &'static str { "float" } }
impl ToProtoType for f64 { fn proto_type() -> &'static str { "double" } }
impl ToProtoType for bool { fn proto_type() -> &'static str { "bool" } }
impl ToProtoType for Vec<u8> { fn proto_type() -> &'static str { "bytes" } }
impl<T: ToProtoType> ToProtoType for Vec<T> {
    fn proto_type() -> &'static str { T::proto_type() } // "repeated T" handled in field gen
}
impl<T: ToProtoType> ToProtoType for Option<T> {
    fn proto_type() -> &'static str { T::proto_type() } // "optional T" handled in field gen
}
```

For user-defined structs, provide a derive macro `#[derive(ToProtoType)]` that generates the message definition from struct fields:

```rust
#[derive(ToProtoType)]
struct User {
    id: u32,        // → uint32 id = 1;
    name: String,   // → string name = 2;
    email: String,  // → string email = 3;
}

// Generates:
// message User {
//   uint32 id = 1;
//   string name = 2;
//   string email = 3;
// }
```

#### Step 2: API Type → .proto File

Define a trait `ApiToProto` (similar to `ApiToSpec`):

```rust
pub trait ApiToProto {
    fn to_proto(service_name: &str) -> String;
}
```

This walks the API tuple and generates:

```protobuf
syntax = "proto3";
package myapi;

service MyService {
    // GET /users → ListUsers (server streaming or unary)
    rpc ListUsers(ListUsersRequest) returns (ListUsersResponse);

    // GET /users/:id → GetUser
    rpc GetUser(GetUserRequest) returns (User);

    // POST /users → CreateUser
    rpc CreateUser(CreateUserRequest) returns (User);

    // DELETE /users/:id → DeleteUser
    rpc DeleteUser(DeleteUserRequest) returns (google.protobuf.Empty);
}

message ListUsersRequest {}

message ListUsersResponse {
    repeated User users = 1;
}

message GetUserRequest {
    uint32 id = 1;  // from path capture
}

message CreateUserRequest {
    string name = 1;   // from request body fields
    string email = 2;
}

message DeleteUserRequest {
    uint32 id = 1;  // from path capture
}

message User {
    uint32 id = 1;
    string name = 2;
    string email = 3;
}
```

**Mapping rules:**
- HTTP method → RPC type:
  - GET with no captures → unary RPC returning a list wrapper
  - GET with captures → unary RPC with captures as request fields
  - POST/PUT/PATCH → unary RPC with body fields + captures as request fields
  - DELETE → unary RPC returning `google.protobuf.Empty`
- Path captures → request message fields
- Request body type → request message fields (flattened)
- Response type → response message (or existing message if it's a domain type)
- Handler name → RPC method name (snake_case → PascalCase)

#### Step 3: Generate Tonic Service Trait

Given the .proto, also generate the Tonic service trait in Rust so the user can implement it alongside their typeway handlers:

```rust
pub trait MyServiceGrpc: Send + Sync + 'static {
    async fn list_users(&self, request: tonic::Request<ListUsersRequest>)
        -> Result<tonic::Response<ListUsersResponse>, tonic::Status>;

    async fn get_user(&self, request: tonic::Request<GetUserRequest>)
        -> Result<tonic::Response<User>, tonic::Status>;

    // ...
}
```

Or better: generate a bridge that reuses the existing typeway handlers:

```rust
/// Bridge that adapts typeway handlers to serve gRPC requests.
/// Uses the same handler functions for both REST and gRPC.
pub struct GrpcBridge<S> {
    state: S,
}

#[tonic::async_trait]
impl<S: Clone + Send + Sync + 'static> MyServiceGrpc for GrpcBridge<S> {
    async fn get_user(&self, request: tonic::Request<GetUserRequest>)
        -> Result<tonic::Response<User>, tonic::Status>
    {
        // Extract path captures from the gRPC request
        let id = request.into_inner().id;
        // Call the same handler logic
        let user = handlers::get_user_impl(id, &self.state).await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(user))
    }
}
```

This is the dream: same business logic, two protocols.

#### Step 4: Runtime Proto Serving

Add a convenience method:

```rust
Server::<API>::new(handlers)
    .with_grpc::<MyServiceGrpc>(grpc_impl)
    .serve(addr)
    .await?;
```

This uses the side-by-side serving pattern from Part 1, but wired up automatically.

### Implementation Phases

#### Phase A: Type mapping + .proto generation (MVP)

- `ToProtoType` trait with primitive impls
- `#[derive(ToProtoType)]` for user structs
- `ApiToProto` trait walking API tuples
- CLI command: `typeway-grpc generate-proto --output service.proto`
- Test: generate .proto from a simple API type, validate syntax

#### Phase B: Tonic service generation

- Generate Tonic service trait from the API type
- `GrpcBridge` adapter reusing handler logic
- Test: compile the generated trait, implement it, serve requests

#### Phase C: Unified serving

- `with_grpc()` on Server for automatic multiplexing
- Shared middleware application
- Test: hit the same server with REST and gRPC clients

#### Phase D: Build script integration

- `build.rs` helper that generates .proto at build time from the API type
- `prost-build` integration to generate Rust types from the .proto
- Full cycle: API type → .proto → Rust types → Tonic service

### Dependencies

```toml
[dependencies]
typeway-core = { path = "../typeway-core" }
tonic = { version = "0.12", optional = true }
prost = { version = "0.13", optional = true }
prost-types = { version = "0.13", optional = true }
```

### Open Questions

1. **Streaming RPCs**: Should GET endpoints that return `Vec<T>` map to server-streaming RPCs? This is natural for gRPC but changes the handler interface. Possibly opt-in via a marker type.

2. **Error mapping**: Typeway uses `JsonError` with HTTP status codes. gRPC uses `tonic::Status` with gRPC status codes (OK, NOT_FOUND, INTERNAL, etc.). The bridge needs a mapping. Could be a trait: `impl IntoGrpcStatus for JsonError`.

3. **Auth**: `Protected<Auth, E>` on the REST side → gRPC metadata interceptor on the Tonic side. The auth extractor needs a gRPC-aware variant.

4. **Field numbering**: Proto field numbers must be stable across versions. When `#[derive(ToProtoType)]` generates numbers from struct field order, renaming or reordering fields breaks wire compatibility. Consider using `#[proto(tag = N)]` attributes or generating a `.proto` file that the user maintains.

5. **Nested messages**: When a request body contains nested structs (`CreateArticle { author: Author, tags: Vec<Tag> }`), the proto generator needs to recursively emit message definitions. This requires the `ToProtoType` trait to report sub-messages.

---

## Part 3: Bidirectional Embedding

Both directions work because both typeway and Tonic speak Tower.

### Embed Tonic inside typeway

Tonic produces a `tower::Service`. Typeway's `Server::with_fallback()` accepts any Tower service. gRPC traffic (identified by `content-type: application/grpc`) falls through to Tonic:

```rust
let grpc = tonic::transport::Server::builder()
    .add_service(my_grpc_service)
    .into_service();

Server::<API>::new(handlers)
    .with_fallback(grpc)
    .serve(addr).await?;
```

### Embed typeway inside Tonic

Provide `Server::into_tonic_service()` (like `into_axum_router()`) that wraps the typeway router as a Tower service Tonic can embed:

```rust
let rest = typeway_server.into_tonic_service();

tonic::transport::Server::builder()
    .add_service(my_grpc_service)
    .add_routes(rest)
    .serve(addr).await?;
```

The body type conversion (Tonic uses `http-body::BoxBody`, typeway uses its own `BoxBody`) is the same adapter problem solved for Axum interop.

---

## Part 4: Migration Tool (`typeway-grpc` CLI)

Bidirectional code generation between `.proto` files and typeway API types.

### .proto → typeway API type

```
typeway-grpc api-from-proto --file service.proto --output src/api.rs
```

Reads a `.proto` file (using `protobuf-parse` or a hand-written parser — proto syntax is simple) and generates:
- `typeway_path!` declarations for each RPC method (mapped to REST paths)
- Endpoint types in an API tuple
- Rust structs for each proto message with `#[derive(Serialize, Deserialize, ToProtoType)]`
- `ToProtoType` impls with correct field tags

### typeway API type → .proto

```
typeway-grpc proto-from-api --file src/api.rs --output service.proto
```

This is Phase A of the implementation — reads a typeway API type (via `syn` parsing, same approach as `typeway-migrate`) and emits a `.proto` file with:
- Service definition with one RPC per endpoint
- Message definitions for request/response types
- Field numbering from struct field order or `#[proto(tag = N)]` attributes

### Mapping rules

| typeway | gRPC |
|---|---|
| `GET /users` | `rpc ListUsers(Empty) returns (ListUsersResponse)` |
| `GET /users/:id` | `rpc GetUser(GetUserRequest) returns (User)` |
| `POST /users` with `Json<CreateUser>` | `rpc CreateUser(CreateUser) returns (User)` |
| `DELETE /users/:id` | `rpc DeleteUser(DeleteUserRequest) returns (Empty)` |
| Path capture `u32` | Request message field `uint32` |
| `Vec<T>` response | Wrapper message with `repeated T` |
| `Protected<Auth, E>` | Noted as requiring auth metadata (comment in .proto) |

---

## Part 5: Resolved Design Decisions

1. **Streaming RPCs**: Punt to a `StreamingEndpoint` marker type. Don't auto-detect from `Vec<T>` return types — that changes the handler interface. Explicit opt-in only.

2. **Field numbering**: Require `#[proto(tag = N)]` attributes for stability. `#[derive(ToProtoType)]` generates initial numbers from field order, but the user must maintain them for wire compatibility.

3. **Error mapping**: Provide `impl IntoGrpcStatus for JsonError` mapping HTTP status codes to gRPC status codes (404 → NOT_FOUND, 401 → UNAUTHENTICATED, 500 → INTERNAL, etc.).

4. **Auth**: `Protected<Auth, E>` maps to a gRPC metadata interceptor. The `GrpcBridge` checks metadata for the auth token the same way the REST extractor checks the Authorization header.

---

## Implementation Order

Start with Phase A — .proto generation is useful standalone and validates the type mapping. Each subsequent phase delivers independent value:

- **Phase A**: `ToProtoType` trait, `ApiToProto`, CLI `proto-from-api` command
- **Phase B**: Tonic service trait generation, `GrpcBridge` adapter
- **Phase C**: `with_grpc()` on Server, bidirectional embedding helpers
- **Phase D**: Build script, `api-from-proto` CLI command (proto parser)

This is a v0.2 feature. It doesn't block v0.1 publishing.
