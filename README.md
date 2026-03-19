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
| `full` | no | server + client + openapi |

## Tower Middleware

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

## OpenAPI

Enable the `openapi` feature to serve an auto-generated OpenAPI spec and Swagger UI:

```rust
Server::<API>::new(handlers)
    .with_openapi("My API", "1.0.0")
    .serve(addr)
    .await?;
// GET /openapi.json — the spec
// GET /docs         — Swagger UI
```

## Axum Interoperability

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

## Type-Safe Client

With the `client` feature, call endpoints using the same types as the server:

```rust
let client = Client::new("http://localhost:3000")?;

// Fully type-checked — path captures, request body, and response type
// are all verified against the endpoint definition.
let user = client.call::<GetEndpoint<UserByIdPath, User>>((42u32,)).await?;
```

## Workspace Structure

| Crate | Description |
|-------|-------------|
| `typeway` | Facade crate — re-exports everything |
| `typeway-core` | Type-level primitives (path segments, methods, HList) |
| `typeway-server` | Tower/Hyper server integration |
| `typeway-client` | Type-safe HTTP client |
| `typeway-openapi` | OpenAPI 3.1 spec derivation |
| `typeway-macros` | Proc macros (`typeway_path!`, `#[handler]`, `#[api_description]`) |

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

This is directly inspired by Haskell's [Servant](https://docs.servant.dev/en/stable/), which pioneered the idea of APIs as types. Typeway brings that idea to Rust without requiring nightly features, GATs, or const generics for strings.

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

### Zero Ceremony Ecosystem Integration

Typeway doesn't ask you to choose between it and the existing Tower/Axum ecosystem. It composes with both:

- **Tower middleware** works directly via `.layer()` — CorsLayer, TraceLayer, TimeoutLayer, your own custom layers
- **Axum interop** is bidirectional: nest typeway inside Axum (`into_axum_router()`), or nest Axum inside typeway (`with_axum_fallback()`)
- **Hyper 1.x** is the transport layer — no custom HTTP implementation

You can adopt typeway for part of your API and keep the rest in Axum. Or start with Axum and gradually migrate endpoints to typeway for stronger type guarantees. No all-or-nothing commitment.

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
| Stable Rust | Yes | Yes | Yes | Yes | N/A |

### How Typeway Improves on Servant

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

- **Gradual migration.** Nest a typeway API inside an existing Axum application at `/api/v2` while the rest of the app stays unchanged. Migrate endpoints one at a time. No big-bang rewrite.
- **Ecosystem access.** Every Axum extractor, every Axum middleware, every Axum tutorial and blog post is potentially relevant. If someone has solved a problem in Axum, the solution likely works with typeway's router too, since both speak Tower.
- **Risk reduction.** Adopting a new framework is risky. Axum interop means you're never locked in — if typeway doesn't work for a particular endpoint, fall back to Axum for that route and keep typeway for the rest. The cost of trying typeway is near zero.
- **Bidirectional embedding.** This isn't just "typeway can call Axum." It's fully bidirectional: Axum routes can be the fallback inside a typeway server. This means you can use Axum's mature WebSocket handling, static file serving, or any other Axum feature alongside typeway's type-safe endpoints.

No other type-safe web framework offers this kind of ecosystem integration. Servant can't embed a WAI app inside a Servant server. Dropshot has no interop with Axum or Actix. Typeway's tower-native architecture makes it a participant in the ecosystem rather than an island.
