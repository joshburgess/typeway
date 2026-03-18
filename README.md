# Wayward

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

## Comparison

| Feature | Wayward | Axum | Warp | Dropshot |
|---------|---------|------|------|----------|
| API-as-type | Yes | No | No | Partial |
| Compile-time handler verification | Yes | No | No | Yes |
| Type-safe client from API type | Yes | No | No | No |
| OpenAPI from types | Yes | No | No | Yes |
| Tower middleware | Yes | Yes | No | No |
| Axum interop | Yes | — | No | No |
