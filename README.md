# Typeway

[![CI](https://github.com/joshburgess/typeway/actions/workflows/ci.yml/badge.svg)](https://github.com/joshburgess/typeway/actions/workflows/ci.yml)

A type-level web framework for Rust where your entire API is described as a type.

Servers, clients, and OpenAPI schemas are all derived from that single type definition. If the types compile, the pieces fit together.

Built on Tokio, Tower, and Hyper — fully compatible with the Axum ecosystem.

## Quick Start

```rust
use typeway::prelude::*;

// 1. Define path types
typeway_path!(type HelloPath = "hello");
typeway_path!(type GreetPath = "greet" / String);

// 2. Define the API as a type
type API = (
    GetEndpoint<HelloPath, String>,
    GetEndpoint<GreetPath, String>,
);

// 3. Write handlers
async fn hello() -> &'static str { "Hello, world!" }
async fn greet(path: Path<GreetPath>) -> String {
    let (name,) = path.0;
    format!("Hello, {name}!")
}

// 4. Serve — the compiler verifies every endpoint has a handler
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Server::<API>::new((
        bind!(hello),
        bind!(greet),
    ))
    .serve("0.0.0.0:3000".parse()?)
    .await?;
    Ok(())
}
```

## Core Idea

The API specification is a Rust type — a tuple of endpoint descriptors:

```rust
type UsersAPI = (
    GetEndpoint<UsersPath, Json<Vec<User>>>,       // GET /users
    GetEndpoint<UserByIdPath, Json<User>>,          // GET /users/:id
    PostEndpoint<UsersPath, Json<CreateUser>, Json<User>>,  // POST /users
    DeleteEndpoint<UserByIdPath, StatusCode>,       // DELETE /users/:id
);
```

This single type drives:
- **Server** — compile-time verification that every endpoint has a handler
- **Client** — type-safe HTTP calls derived from the same endpoints
- **OpenAPI** — spec generated at startup from the type

## Installation

```toml
[dependencies]
typeway = "0.1"

# Optional features:
# typeway = { version = "0.1", features = ["client"] }       # type-safe HTTP client
# typeway = { version = "0.1", features = ["openapi"] }      # OpenAPI spec generation
# typeway = { version = "0.1", features = ["axum-interop"] } # Axum interoperability
# typeway = { version = "0.1", features = ["full"] }         # server + client + openapi
```

## Feature Flags

| Flag | Default | Description |
|------|---------|-------------|
| `server` | yes | HTTP/1.1 + HTTP/2 server (Tower/Hyper) |
| `client` | no | Type-safe HTTP client (reqwest) |
| `openapi` | no | OpenAPI 3.1 spec generation + embedded docs UI |
| `axum-interop` | no | Embed typeway in Axum apps and vice versa |
| `tls` | no | HTTPS via tokio-rustls |
| `ws` | no | WebSocket upgrade support |
| `multipart` | no | Multipart form upload (file uploads) |
| `grpc` | no | gRPC server, client, .proto generation, server reflection, health check (Tonic) |
| `full` | no | server + client + openapi |

## Workspace Structure

| Crate | Description |
|-------|-------------|
| `typeway` | Facade crate — re-exports everything |
| `typeway-core` | Type-level primitives (path segments, methods, HList) |
| `typeway-server` | Tower/Hyper server integration |
| `typeway-client` | Type-safe HTTP client |
| `typeway-openapi` | OpenAPI 3.1 spec derivation |
| `typeway-macros` | Proc macros (`typeway_path!`, `#[handler]`, `#[api_description]`) |
| `typeway-grpc` | gRPC/Protocol Buffers generation and serving via Tonic |

## What Makes Typeway Different

### The API Is the Type

Most Rust web frameworks build the API imperatively — you register routes one at a time with a router, and the relationship between routes, handlers, and documentation exists only in the programmer's head. Typeway inverts this: the API is declared as a single Rust type, and everything else is derived from it.

```rust
type API = (
    GetEndpoint<UsersPath, Json<Vec<User>>>,
    PostEndpoint<UsersPath, Json<CreateUser>, Json<User>>,
    DeleteEndpoint<UserByIdPath, StatusCode>,
);
```

This isn't a DSL or a macro that generates code behind your back. It's a plain Rust type alias. The compiler understands it, IDE tooling works with it, and you can inspect it in `cargo doc`. The server, client, and OpenAPI spec are all projections of this one type.

This is directly inspired by Haskell's [Servant](https://docs.servant.dev/en/stable/), which pioneered the idea of APIs as types. Typeway brings that idea to Rust on stable, working around the absence of const generic `&'static str` parameters (still unstable) by using marker types with a `LitSegment` trait. See [Type-Level Design](TYPE-LEVEL-DESIGN.md) for a detailed analysis of the Rust type system features Typeway uses, what it works around, and how future language improvements could simplify the framework.

### Compile-Time Handler Completeness

In Axum, if you forget to register a handler for a route, you get a 404 at runtime. In typeway, you get a compile error:

```rust
// API has 3 endpoints but you only provided 2 handlers — doesn't compile
Server::<API>::new((
    bind!(list_users),
    bind!(get_user),
    // missing: create_user  ← compiler error here
))
```

The `Serves<API>` trait enforces that the handler tuple has exactly the right number of `BoundHandler<E>` entries, one per endpoint. No more, no less. This is checked entirely at compile time with zero runtime cost.

### Single Source of Truth for Server + Client + OpenAPI

Most frameworks require you to maintain the API definition in multiple places: route registrations in the server, HTTP calls in the client, and annotations or YAML files for OpenAPI. These inevitably drift apart.

Typeway derives all three from the same type:

```rust
// Server: compile-time verified handlers
Server::<API>::new(handlers).serve(addr).await?;

// Client: type-safe calls using the same endpoint types
let user = client.call::<GetEndpoint<UserByIdPath, User>>((42u32,)).await?;

// OpenAPI: spec generated from the type — no annotations needed
Server::<API>::new(handlers).with_openapi("My API", "1.0").serve(addr).await?;
```

If you change the API type, the compiler forces you to update all three. There is no YAML to forget.

### Type-Level Path Encoding via HLists

URL paths are encoded as heterogeneous lists at the type level:

```rust
// /users/:id/posts/:post_id becomes:
HCons<Lit<users>, HCons<Capture<u32>, HCons<Lit<posts>, HCons<Capture<u32>, HNil>>>>

// Ergonomic macro form:
typeway_path!(type UserPostsPath = "users" / u32 / "posts" / u32);
```

This is a type-level catamorphism (fold) — the `PathSpec` trait recurses over the HList to compute the capture tuple type. A path with captures `u32` and `String` produces `Captures = (u32, String)` at compile time. The runtime path parser is structurally derived from the same type.

Why HLists instead of flat tuples? Paths are inherently recursive: match one segment, then recurse on the remainder. HLists give O(n) trait impls via structural recursion, where flat tuples would require combinatorial explosion of impls for every segment combination.

### Structured Errors as Types, Not Strings

Handler errors are part of the type system. Return `Result<Json<User>, JsonError>` and the framework handles serialization:

```rust
async fn get_user(path: Path<UserByIdPath>) -> Result<Json<User>, JsonError> {
    let (id,) = path.0;
    db.get(id)
      .ok_or_else(|| JsonError::not_found(format!("user {id} not found")))
}
// Produces: {"error": {"status": 404, "message": "user 42 not found"}}
```

Custom extractors can use the same error type, so a missing auth token produces a structured 401 response, not a raw string.

## Server Features

### Tower Middleware

Typeway supports the full Tower middleware ecosystem:

```rust
use typeway::tower_http::cors::CorsLayer;
use typeway::tower_http::timeout::TimeoutLayer;

Server::<API>::new(handlers)
    .layer(CorsLayer::permissive())
    .layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        Duration::from_secs(30),
    ))
    .serve(addr)
    .await?;
```

### OpenAPI

Enable the `openapi` feature to serve an auto-generated OpenAPI spec and Swagger UI:

```rust
Server::<API>::new(handlers)
    .with_openapi("My API", "1.0.0")
    .serve(addr)
    .await?;
// GET /openapi.json — the spec
// GET /docs         — Swagger UI
```

### Axum Interoperability

Embed typeway APIs in Axum apps:

```rust
let typeway_api = Server::<API>::new(handlers);
let app = axum::Router::new()
    .nest("/api/v1", typeway_api.into_axum_router())
    .route("/health", get(|| async { "ok" }));
```

Or embed Axum routes in typeway:

```rust
let axum_routes = axum::Router::new()
    .route("/health", get(|| async { "ok" }));

Server::<API>::new(handlers)
    .with_axum_fallback(axum_routes)
    .serve(addr)
    .await?;
```

### Zero Ceremony Ecosystem Integration

Typeway doesn't ask you to choose between it and the existing Tower/Axum ecosystem. It composes with both:

- **Tower middleware** works directly via `.layer()` — CorsLayer, TraceLayer, TimeoutLayer, your own custom layers
- **Axum interop** is bidirectional: nest typeway inside Axum (`into_axum_router()`), or nest Axum inside typeway (`with_axum_fallback()`)
- **Hyper 1.x** is the transport layer — no custom HTTP implementation

You can adopt typeway for part of your API and keep the rest in Axum. Or start with Axum and gradually migrate endpoints to typeway for stronger type guarantees. No all-or-nothing commitment.

## Type-Safe Client

With the `client` feature, call endpoints using the same types as the server:

```rust
let client = Client::new("http://localhost:3000")?;

// Fully type-checked — path captures, request body, and response type
// are all verified against the endpoint definition.
let user = client.call::<GetEndpoint<UserByIdPath, User>>((42u32,)).await?;
```

## Advanced Type-Level Features

### Session-Typed WebSocket Routes

With the `ws` feature, WebSocket connections can be governed by a session type that encodes the exact sequence of messages the server and client must exchange. Each `.send()` or `.recv()` consumes the channel and returns it at the next protocol state. Sending a message out of order is a compile error — the old channel state has been moved.

Define a protocol as a type:

```rust
use typeway::session::*;

// Server-side protocol: send greeting, receive name, send welcome, done.
type GreetProtocol = Send<String, Recv<String, Send<String, End>>>;
```

Write a handler that the compiler forces to follow the protocol:

```rust
use typeway::typed_ws::TypedWebSocket;

async fn greet_handler(ws: TypedWebSocket<GreetProtocol>) {
    let ws = ws.send("Hello! What is your name?".into()).await.unwrap();
    let (name, ws) = ws.recv().await.unwrap();
    let ws = ws.send(format!("Welcome, {name}!")).await.unwrap();
    ws.close().await.unwrap();
}
```

If you try to call `ws.recv()` when the protocol says `Send`, or call `ws.send()` after the protocol has reached `End`, the code does not compile. Rust's ownership system enforces linearity: the old channel state is consumed by each operation, so there is no way to reuse it in the wrong state.

The `Dual` trait computes the mirror protocol automatically. If the server's protocol is `Send<String, Recv<String, End>>`, then `<GreetProtocol as Dual>::Output` is `Recv<String, Send<String, End>>` — the client's view. This means a single protocol definition drives both sides, and the type system guarantees they are compatible.

Branching is supported via `Offer<L, R>` (the remote peer chooses) and `Select<L, R>` (the local side chooses). Recursive protocols use `Rec<Body>` and `Var` to express loops without infinite types.

**Why it matters:** Standard WebSocket APIs are untyped message pipes. You can send any message at any time, and protocol violations are runtime bugs. Session types make the protocol a compile-time contract. Rust's move semantics give you this for free — no linear type extension needed.

### Content Negotiation

The `NegotiatedResponse<T, Formats>` type lets a single handler return a domain value that is automatically serialized into the best format based on the client's `Accept` header:

```rust
use typeway::negotiate::*;

#[derive(serde::Serialize)]
struct User { id: u32, name: String }

impl std::fmt::Display for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "User({}, {})", self.id, self.name)
    }
}

async fn get_user(accept: AcceptHeader) -> NegotiatedResponse<User, (JsonFormat, TextFormat)> {
    let user = User { id: 1, name: "Alice".into() };
    negotiated(user, accept)
}
```

When a client sends `Accept: application/json`, the response is JSON with `Content-Type: application/json`. When it sends `Accept: text/plain`, the response uses the `Display` impl. Wildcard (`*/*`) and quality-weighted Accept headers are handled correctly, falling back to the first format in the tuple when no preference is expressed.

The format list is a type-level tuple of marker types (`JsonFormat`, `TextFormat`, `HtmlFormat`, `CsvFormat`, or custom formats implementing `ContentFormat`). The `RenderAs<Format>` trait connects a domain type to a specific serialization. Blanket impls cover `RenderAs<JsonFormat>` for any `T: Serialize` and `RenderAs<TextFormat>` for any `T: Display`, so most types work out of the box.

**Why it matters:** Without content negotiation, you either hardcode JSON everywhere or write manual `Accept` header parsing in every handler. `NegotiatedResponse` makes multi-format APIs a type-level declaration — add a format to the tuple and implement `RenderAs` for it, and every handler using that tuple gains the new format automatically.

### Type-Level API Versioning

API evolution is expressed as typed deltas: V2 is defined as a set of changes applied to V1 — added endpoints, removed endpoints, replaced endpoints, and deprecated endpoints. The type system tracks what changed and can verify backward compatibility at compile time.

```rust
use typeway::versioning::*;

// V1 API
type V1 = (
    GetEndpoint<UsersPath, Json<Vec<UserV1>>>,
    GetEndpoint<UserByIdPath, Json<UserV1>>,
    PostEndpoint<UsersPath, Json<CreateUser>, Json<UserV1>>,
);

// V2 changes: add a profile endpoint, replace the user response type, deprecate create
type V2Changes = (
    Added<GetEndpoint<UserProfilePath, Json<Profile>>>,
    Replaced<
        GetEndpoint<UserByIdPath, Json<UserV1>>,
        GetEndpoint<UserByIdPath, Json<UserV2>>,
    >,
    Deprecated<PostEndpoint<UsersPath, Json<CreateUser>, Json<UserV1>>>,
);

// The resolved V2 API after applying changes
type V2Resolved = (
    GetEndpoint<UsersPath, Json<Vec<UserV1>>>,
    GetEndpoint<UserByIdPath, Json<UserV2>>,          // replaced
    PostEndpoint<UsersPath, Json<CreateUser>, Json<UserV1>>,  // deprecated but present
    GetEndpoint<UserProfilePath, Json<Profile>>,      // added
);

type V2 = VersionedApi<V1, V2Changes, V2Resolved>;

// Compile-time check: every V1 endpoint that wasn't replaced still exists in V2
assert_api_compatible!(
    (GetEndpoint<UsersPath, Json<Vec<UserV1>>>,
     PostEndpoint<UsersPath, Json<CreateUser>, Json<UserV1>>),
    V2Resolved
);
```

The `assert_api_compatible!` macro uses a type-level set membership check (`HasEndpoint<E, Idx>`) with an index witness technique: each tuple position gets a distinct type-level index (`Here`, `There<Here>`, `There<There<Here>>`, ...), so the compiler can prove an endpoint exists in the tuple without ambiguity. If you remove an endpoint from V2 that the compatibility check expects, the code does not compile.

The `ApiChangelog` trait provides runtime introspection of the change set — how many endpoints were added, removed, replaced, or deprecated — for documentation tooling and migration reports.

**Why it matters:** API versioning is typically a runtime or documentation concern — you maintain separate route registrations for V1 and V2 and hope they stay consistent. With typed deltas, the relationship between versions is encoded in the type system. Breaking changes are visible in the types, and backward compatibility is a compile-time assertion, not a test you might forget to write.

## gRPC / Protocol Buffers

With the `grpc` feature, the same API type that drives your REST server, client, and OpenAPI spec also generates Protocol Buffers service definitions, serves gRPC alongside REST, provides a type-safe gRPC client, exposes server reflection for tools like `grpcurl`, and runs a health check service with graceful shutdown support.

One API type, seven projections: REST server, REST client, OpenAPI spec, gRPC server, gRPC client, `.proto` file, and server reflection.

### Message Types from Rust Structs

`#[derive(ToProtoType)]` generates Protocol Buffers message definitions directly from Rust structs, with field tags specified via `#[proto(tag = N)]`:

```rust
use typeway_grpc::ToProtoType;

#[derive(ToProtoType)]
struct User {
    #[proto(tag = 1)]
    id: u32,
    #[proto(tag = 2)]
    name: String,
}
```

No hand-written `.proto` message definitions needed — the Rust struct is the source of truth.

### Generating `.proto` Files

Generate a `.proto` file from your API type:

```rust
use typeway_grpc::ApiToProto;

let proto = API::to_proto("UserService", "users.v1");
std::fs::write("users.proto", proto)?;
```

### Serving gRPC and REST Together

Serve gRPC alongside REST on the same port:

```rust
Server::<API>::new(handlers)
    .with_grpc("UserService", "users.v1")
    .serve(addr)
    .await?;
```

The gRPC layer uses Tonic under the hood, sharing the same Tower middleware stack and Tokio runtime as the REST server. Handlers are reused — a single handler implementation serves both REST and gRPC requests.

### Server Reflection and Health Checks

Server reflection is enabled automatically, so tools like `grpcurl` and `grpcui` can discover services at runtime without a `.proto` file on disk:

```sh
grpcurl -plaintext localhost:3000 list
# users.v1.UserService
```

The health check service supports graceful shutdown — it reports `SERVING` while the server is running and transitions to `NOT_SERVING` during shutdown, giving load balancers time to drain connections.

### Server-Streaming RPCs

`ServerStream<E>` supports server-streaming RPCs where a single request produces a stream of responses:

```rust
type API = (
    GetEndpoint<EventsPath, ServerStream<Event>>,
);
```

### Type-Safe gRPC Client

The `grpc_client!` macro generates a type-safe gRPC client from the same endpoint types used by the server:

```rust
use typeway_grpc::grpc_client;

grpc_client! {
    pub struct UserServiceClient;
    service = "UserService";
    package = "users.v1";

    list_users => GetEndpoint<UsersPath, Json<Vec<User>>>;
    get_user => GetEndpoint<UserByIdPath, Json<User>>;
}

let client = UserServiceClient::connect("http://localhost:3000").await?;
let user = client.get_user(42u32).await?;
```

Both REST and gRPC clients are derived from the same API type — change the type, and both clients update.

### Error Mapping

The `IntoGrpcStatus` trait maps handler errors to gRPC status codes, so error types work consistently across REST and gRPC:

```rust
impl IntoGrpcStatus for MyError {
    fn into_grpc_status(self) -> tonic::Status {
        match self {
            MyError::NotFound(msg) => Status::not_found(msg),
            MyError::Internal(msg) => Status::internal(msg),
        }
    }
}
```

### Importing Existing `.proto` Files

For the reverse direction — importing an existing `.proto` file and generating Typeway API types — the `typeway-grpc` CLI provides:

```sh
typeway-grpc api-from-proto --file service.proto
```

This outputs the `typeway_path!` declarations, endpoint types, and request/response struct definitions corresponding to the proto service, giving you a starting point for a type-safe Typeway server that matches an existing gRPC contract.

## Performance: The Cost of Type Safety

Type-level frameworks invite skepticism about runtime cost. Typeway's type erasure and extractor machinery add overhead — but we've measured it, and it's negligible compared to real handler work.

### Dispatch Overhead

Typeway stores handlers as type-erased closures (`Box<dyn Fn(Parts, Bytes) -> Pin<Box<dyn Future>>>`). This is the cost of putting heterogeneous handlers in a single router. Here's what that costs:

| Benchmark | Time | What it measures |
|-----------|------|------------------|
| Direct async fn (no framework) | 0.79 ns | Baseline — raw function call |
| BoxedHandler dispatch | 878 ns | Two heap allocs (closure box + future box) + virtual call |
| + Path extractor | +290 ns | `FromStr::parse` + `Extensions::get` |
| + State extractor | +310 ns | `Extensions::get` + `Clone` |
| + Path + State together | +610 ns | Multiple extractors compose linearly |
| + JSON body parse (16 B) | +230 ns | `serde_json::from_slice` on pre-collected bytes |
| Bytes::clone (any size) | 14 ns | O(1) — reference counted, no copy |

The **878 ns dispatch floor** is the framework's fixed cost per request. Everything else — extractors, serialization, your handler logic — adds on top linearly.

### What This Means in Practice

A typical JSON API handler that queries a database and returns a response takes **1–100 ms**. The 878 ns dispatch overhead is **0.001–0.09%** of that. You cannot measure it in production.

The extractor costs (~300 ns each) are dominated by `TypeId`-keyed hashmap lookups in `http::Extensions`. These are the same lookups Axum performs — typeway doesn't add extra indirection beyond what any extractor-based framework does.

Body bytes are reference-counted (`Bytes`), so passing the pre-collected body to handlers costs 14 ns regardless of payload size. There is no copy.

### Where Typeway Is Slower Than Axum

Typeway's router uses a linear scan with method indexing and first-segment prefix rejection. Axum uses a radix trie (`matchit`). For 10 routes, typeway is ~30% slower at route matching:

| Scenario (10 routes) | Axum | Typeway | Ratio |
|----------------------|------|---------|-------|
| First route match | 1.41 µs | 1.90 µs | 1.35x |
| Last route match | 1.43 µs | 1.93 µs | 1.35x |
| Path with captures | 1.66 µs | 2.12 µs | 1.28x |
| No match (404) | 0.98 µs | 1.50 µs | 1.53x |

This gap comes from two sources: (1) linear scan vs trie for route matching, and (2) the Axum adapter layer in the benchmark adds body type conversion overhead. For APIs with fewer than ~100 routes, the linear scan is fast enough that the difference is invisible in end-to-end latency. The method index ensures that only routes with the matching HTTP method are checked, so a 50-route API with 10 GETs and 40 POSTs only scans 10 entries for a GET request.

### The Trade-Off

Typeway trades ~1 µs of dispatch overhead for compile-time guarantees that eliminate entire categories of runtime bugs: missing handlers, mismatched types between server and client, drifted OpenAPI specs. For any API where correctness matters more than shaving microseconds off an already-sub-millisecond overhead, this is a good trade.

## Comparison

| Feature | Typeway | Axum | Warp | Dropshot | Servant (Haskell) |
|---------|---------|------|------|----------|-------------------|
| API-as-type | Yes | No | No | Partial | Yes |
| Compile-time handler completeness | Yes | No | No | Yes | Yes |
| Type-safe client from API type | Yes | No | No | No | Yes |
| OpenAPI from types | Yes | No | No | Yes | Yes (via servant-openapi3) |
| Tower middleware | Yes | Yes | No | No | N/A |
| Axum interop | Yes | — | No | No | N/A |
| Streaming/SSE bodies | Yes | Yes | Yes | No | Limited |
| Session-typed WebSockets | Yes | No | No | No | No |
| Content negotiation (type-level) | Yes | No | No | No | Partial (via servant-content-types) |
| Type-level API versioning | Yes | No | No | No | No |
| gRPC from API type | Yes | No | No | No | No |
| Stable Rust | Yes | Yes | Yes | Yes | N/A |

## How Typeway Improves on Servant

Typeway owes a direct intellectual debt to Haskell's [Servant](https://docs.servant.dev/en/stable/), which pioneered the idea of APIs as types. Servant proved the concept; typeway refines and extends it by leveraging Rust's ecosystem in ways that Haskell's cannot match.

**Built-in vs. fragmented ecosystem.** Servant's power is split across dozens of packages: `servant-server`, `servant-client`, `servant-swagger`, `servant-auth`, `servant-multipart`, `servant-websockets`, each with its own maintainer, version constraints, and compatibility matrix. A real Servant project often pulls in 10+ servant-* packages and navigating their interactions is a source of friction. Typeway ships everything in one workspace: server, client, OpenAPI, auth extractors, WebSocket support, streaming, structured errors, and middleware — all designed to work together from day one. There is no version matrix to debug.

**Middleware is a first-class citizen.** Servant has no standard middleware story. WAI middleware exists, but it operates below Servant's type level — you can't express "this endpoint requires authentication" in the type and have middleware enforce it. Typeway inherits Tower's middleware architecture, where layers compose naturally with `.layer()`, and custom extractors (like `AuthUser`) participate in the type system. A missing auth token is a compile-time extractor error, not a runtime WAI filter.

**Compile time discipline.** Servant is notorious for slow compile times. A 50-endpoint API can take minutes to compile because GHC resolves deeply nested type families and type-class instances at every use site. Typeway attacks this head-on: flat tuple impls generated by `macro_rules!` replace recursive type-class chains, method-indexed routing avoids O(routes) type-level dispatch, and type erasure at the router boundary (`BoundHandler`) prevents monomorphization blowup. The result is compile times that scale linearly, not exponentially.

**Streaming and real-time.** Servant's body handling is synchronous and buffered by default. Streaming requires `servant-conduit` or `servant-pipes` and integrates awkwardly. Typeway has native streaming (`body_from_stream`), Server-Sent Events (`sse_body`), and WebSocket upgrades — all using standard Rust async primitives (tokio streams, futures).

**Error messages are actionable.** Servant's type errors are legendary for their inscrutability — pages of GHC output about overlapping instances and ambiguous type variables. Typeway uses `#[diagnostic::on_unimplemented]` on key traits to produce errors like `` `NotAResponse` cannot be used as an HTTP response `` with a list of valid alternatives. The `#[handler]` macro catches mistakes at the function definition, not at the distant `Server::new` call site.

**Gradual adoption.** There is no equivalent of typeway's Axum interop in Servant's world. You either use Servant for your entire API or you don't use it at all. Typeway lets you nest a single type-safe endpoint group inside an existing Axum application, or add Axum routes as a fallback inside typeway. This makes adoption incremental — you can prove the value on one service boundary before committing to the approach.

## Why Tower, Hyper, and Tokio

Typeway is built on Tower, Hyper, and Tokio — the same foundation as Axum, Tonic (gRPC), and most of the production Rust web ecosystem. This is a deliberate architectural choice, not a default, and it provides concrete benefits that a custom stack would not.

### Tower: Middleware Without Reinvention

Tower's `Service` trait is the `Iterator` of async request/response processing: a universal interface that middleware, load balancers, and service meshes all understand. By implementing `tower::Service` for typeway's router, every Tower-compatible middleware works out of the box:

- **tower-http** gives you CORS, compression, tracing, timeouts, rate limiting, request IDs, and content-type validation — battle-tested layers used in production by thousands of services.
- **Custom middleware** follows the same pattern. Write a `Layer` and it works with typeway, Axum, Tonic, and any other Tower-based framework.
- **Service composition** means you can wrap a typeway API in a retry layer, put it behind a circuit breaker, or load-balance across backends — all without typeway knowing or caring.

The alternative — building a custom middleware system — would mean reimplementing CORS handling, compression negotiation, timeout logic, and every other cross-cutting concern from scratch. Tower's ecosystem represents years of production hardening that typeway inherits for free.

### Hyper: HTTP Without Opinions

Hyper is the de facto Rust HTTP implementation. It handles connection management, HTTP/1.1 and HTTP/2 protocol details, keep-alive, chunked transfer encoding, and upgrade handshakes. Typeway delegates all of this to Hyper rather than reimplementing any of it.

This means:

- **Protocol correctness** — Hyper's HTTP implementation is exhaustively tested and fuzzed. Typeway doesn't need to worry about edge cases in chunked encoding or connection lifecycle.
- **Performance** — Hyper is one of the fastest HTTP implementations in any language. Typeway adds only the routing and extraction layer on top; the hot path of connection handling is pure Hyper.
- **Upgrade support** — WebSocket upgrades, HTTP/2, and future protocol extensions come from Hyper. Typeway's WebSocket support is a thin adapter over Hyper's upgrade mechanism, not a custom implementation.

### Tokio: The Runtime Everyone Already Has

Every non-trivial async Rust application already depends on Tokio. By building on Tokio directly, typeway avoids the dual-runtime problem that plagues frameworks built on `async-std` or custom executors. Your database driver, your Redis client, your message queue consumer, and your typeway server all share one runtime with one set of configuration knobs.

### Axum Interop: The Ecosystem Multiplier

The Axum compatibility layer is the strongest practical argument for this foundation choice. Axum is the most widely adopted Rust web framework, and it sits on the exact same Tower/Hyper/Tokio stack. This creates a unique opportunity:

- **Targeted type safety.** You don't have to use typeway for everything. Use it for the endpoints where type-level correctness matters most — a payment API, a permissions system, an integration contract between services — and keep the rest of your application in Axum. A single project can have hundreds of Axum routes and a handful of typeway routes, all sharing the same middleware, runtime, and binary.

- **Team autonomy.** In organizations with multiple teams contributing to a shared service, typeway and Axum coexist naturally. A team that values functional programming patterns and compile-time guarantees can use typeway for their domain. A team that prefers Axum's imperative routing style keeps working the way they always have. Both teams share the same Tower middleware stack, the same Tokio runtime, and the same binary. No separate services, no microservice overhead — just different routing strategies in the same application.

- **Gradual migration.** Nest a typeway API inside an existing Axum application at `/api/v2` while the rest of the app stays unchanged. Migrate endpoints one at a time. No big-bang rewrite.

- **Ecosystem access.** Every Axum extractor, every Axum middleware, every Axum tutorial and blog post is potentially relevant. If someone has solved a problem in Axum, the solution likely works with typeway's router too, since both speak Tower.

- **Risk reduction.** Adopting a new framework is risky. Axum interop means you're never locked in — if typeway doesn't work for a particular endpoint, fall back to Axum for that route and keep typeway for the rest. The cost of trying typeway is near zero.

- **Bidirectional embedding.** This isn't just "typeway can call Axum." It's fully bidirectional: Axum routes can be the fallback inside a typeway server. This means you can use Axum's mature WebSocket handling, static file serving, or any other Axum feature alongside typeway's type-safe endpoints.

No other type-safe web framework offers this kind of ecosystem integration. Servant can't embed a WAI app inside a Servant server. Dropshot has no interop with Axum or Actix. Typeway's tower-native architecture makes it a participant in the ecosystem rather than an island.
