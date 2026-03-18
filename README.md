# Wayward

[![CI](https://github.com/joshburgess/wayward/actions/workflows/ci.yml/badge.svg)](https://github.com/joshburgess/wayward/actions/workflows/ci.yml)

A type-level web framework for Rust where your entire API is described as a type.

Servers, clients, and OpenAPI schemas are all derived from that single type definition. If the types compile, the pieces fit together.

Built on Tokio, Tower, and Hyper — fully compatible with the Axum ecosystem.

## Quick Start

```rust
use wayward::prelude::*;

// 1. Define path types
wayward_path!(type HelloPath = "hello");
wayward_path!(type GreetPath = "greet" / String);

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
async fn main() {
    Server::<API>::new((
        bind::<_, _, _>(hello),
        bind::<_, _, _>(greet),
    ))
    .serve("0.0.0.0:3000".parse().unwrap())
    .await
    .unwrap();
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
wayward = "0.1"

# Optional features:
# wayward = { version = "0.1", features = ["client"] }       # type-safe HTTP client
# wayward = { version = "0.1", features = ["openapi"] }      # OpenAPI spec generation
# wayward = { version = "0.1", features = ["axum-interop"] } # Axum interoperability
# wayward = { version = "0.1", features = ["full"] }         # server + client + openapi
```

## Feature Flags

| Flag | Default | Description |
|------|---------|-------------|
| `server` | yes | HTTP server (Tower/Hyper) |
| `client` | no | Type-safe HTTP client (reqwest) |
| `openapi` | no | OpenAPI 3.1 spec generation + Swagger UI |
| `axum-interop` | no | Embed wayward in Axum apps and vice versa |
| `full` | no | server + client + openapi |

## Tower Middleware

Wayward supports the full Tower middleware ecosystem:

```rust
use wayward::tower_http::cors::CorsLayer;
use wayward::tower_http::timeout::TimeoutLayer;

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

Embed wayward APIs in Axum apps:

```rust
let wayward_api = Server::<API>::new(handlers);
let app = axum::Router::new()
    .nest("/api/v1", wayward_api.into_axum_router())
    .route("/health", get(|| async { "ok" }));
```

Or embed Axum routes in wayward:

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
| `wayward` | Facade crate — re-exports everything |
| `wayward-core` | Type-level primitives (path segments, methods, HList) |
| `wayward-server` | Tower/Hyper server integration |
| `wayward-client` | Type-safe HTTP client |
| `wayward-openapi` | OpenAPI 3.1 spec derivation |
| `wayward-macros` | Proc macros (`wayward_path!`, `#[handler]`, `#[api_description]`) |

## What Makes Wayward Different

### The API Is the Type

Most Rust web frameworks build the API imperatively — you register routes one at a time with a router, and the relationship between routes, handlers, and documentation exists only in the programmer's head. Wayward inverts this: the API is declared as a single Rust type, and everything else is derived from it.

```rust
type API = (
    GetEndpoint<UsersPath, Json<Vec<User>>>,
    PostEndpoint<UsersPath, Json<CreateUser>, Json<User>>,
    DeleteEndpoint<UserByIdPath, StatusCode>,
);
```

This isn't a DSL or a macro that generates code behind your back. It's a plain Rust type alias. The compiler understands it, IDE tooling works with it, and you can inspect it in `cargo doc`. The server, client, and OpenAPI spec are all projections of this one type.

This is directly inspired by Haskell's [Servant](https://docs.servant.dev/en/stable/), which pioneered the idea of APIs as types. Wayward brings that idea to Rust without requiring nightly features, GATs, or const generics for strings.

### Compile-Time Handler Completeness

In Axum, if you forget to register a handler for a route, you get a 404 at runtime. In wayward, you get a compile error:

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

Wayward derives all three from the same type:

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
wayward_path!(type UserPostsPath = "users" / u32 / "posts" / u32);
```

This is a type-level catamorphism (fold) — the `PathSpec` trait recurses over the HList to compute the capture tuple type. A path with captures `u32` and `String` produces `Captures = (u32, String)` at compile time. The runtime path parser is structurally derived from the same type.

Why HLists instead of flat tuples? Paths are inherently recursive: match one segment, then recurse on the remainder. HLists give O(n) trait impls via structural recursion, where flat tuples would require combinatorial explosion of impls for every segment combination.

### Zero Ceremony Ecosystem Integration

Wayward doesn't ask you to choose between it and the existing Tower/Axum ecosystem. It composes with both:

- **Tower middleware** works directly via `.layer()` — CorsLayer, TraceLayer, TimeoutLayer, your own custom layers
- **Axum interop** is bidirectional: nest wayward inside Axum (`into_axum_router()`), or nest Axum inside wayward (`with_axum_fallback()`)
- **Hyper 1.x** is the transport layer — no custom HTTP implementation

You can adopt wayward for part of your API and keep the rest in Axum. Or start with Axum and gradually migrate endpoints to wayward for stronger type guarantees. No all-or-nothing commitment.

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

## Comparison

| Feature | Wayward | Axum | Warp | Dropshot | Servant (Haskell) |
|---------|---------|------|------|----------|-------------------|
| API-as-type | Yes | No | No | Partial | Yes |
| Compile-time handler completeness | Yes | No | No | Yes | Yes |
| Type-safe client from API type | Yes | No | No | No | Yes |
| OpenAPI from types | Yes | No | No | Yes | Yes (via servant-openapi3) |
| Tower middleware | Yes | Yes | No | No | N/A |
| Axum interop | Yes | — | No | No | N/A |
| Streaming/SSE bodies | Yes | Yes | Yes | No | Limited |
| Stable Rust | Yes | Yes | Yes | Yes | N/A |

### How Wayward Compares to Servant

Wayward is most directly comparable to Haskell's Servant. Both share the core idea: the API is a type, and implementations are derived from it. The key differences:

- **Servant** uses GHC's type-level strings, type operators (`:<|>`, `:>`), and type families. Wayward uses HLists, marker types, and trait-level computation — achieving similar results within Rust's type system.
- **Servant** is known for slow compile times on large APIs due to deep type-level computation. Wayward mitigates this with flat tuple impls (macro-generated for arities 1-16) instead of recursive type-class resolution, and method-indexed routing instead of linear type-level dispatch.
- **Servant** has a mature ecosystem (servant-auth, servant-swagger, servant-client). Wayward has built-in equivalents (auth extractors, OpenAPI generation, type-safe client) rather than separate packages.
- **Wayward** integrates with the Rust async ecosystem (Tower, Hyper, Axum) natively. Servant integrates with WAI/Warp in Haskell.
