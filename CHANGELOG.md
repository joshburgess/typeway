# Changelog

## 0.1.0 — Initial Release

### Core Framework
- **API-as-type**: define your entire HTTP API as a Rust type — a tuple of endpoint descriptors
- **Compile-time handler verification**: `Serves<API>` ensures every endpoint has a handler
- **Type-level path encoding**: HList-based path segments with `Lit<S>` and `Capture<T>`
- **Endpoint types**: `GetEndpoint`, `PostEndpoint`, `PutEndpoint`, `DeleteEndpoint`, `PatchEndpoint` with optional query parameter type

### Server (`wayward-server`)
- Tower/Hyper 1.x server with `Server::serve()` and `Server::serve_with_shutdown()`
- **Extractors**: `Path<P>`, `State<T>`, `Query<T>`, `Json<T>`, `Extension<T>`, `Header<T>`, `HeaderMap`, `http::Method`, `http::Uri`, `Bytes`, `String`
- **Responses**: `IntoResponse` trait with impls for `&str`, `String`, `Json<T>`, `StatusCode`, `(StatusCode, T)`, `Result<T, E>`, `Bytes`, `Response<BoxBody>`
- **Streaming**: `body_from_stream()` and `sse_body()` for chunked/SSE responses
- **Body size limits**: configurable via `Server::max_body_size()`, default 2 MiB
- **Structured errors**: `JsonError` with convenience constructors and JSON serialization
- **Route nesting**: `Server::nest("/prefix")` for path prefix grouping
- **Tower middleware**: `.layer()` supports any Tower layer (CORS, timeout, compression, tracing)
- **Request ID**: `RequestIdLayer` generates UUID v4 per request
- **Fallback**: `Server::with_fallback()` for Tower service fallback on unmatched routes
- **Graceful shutdown**: `Server::serve_with_shutdown(listener, signal)`

### Macros (`wayward-macros`)
- `wayward_path!`: ergonomic path type definitions (`wayward_path!(type P = "users" / u32)`)
- `wayward_api!`: inline API type definitions with method/path/body syntax
- `#[handler]`: validates handler functions at definition site
- `#[api_description]`: trait-based API definition with auto-generated endpoint types
- `bind!()`: binds handlers without turbofish boilerplate

### Client (`wayward-client`)
- Type-safe HTTP client derived from the same API types as the server
- `Client::call::<Endpoint>(args)` with compile-time verification
- Supports all HTTP methods, path captures, and request/response bodies

### OpenAPI (`wayward-openapi`)
- OpenAPI 3.1 spec generation from API types at startup
- Embedded docs UI at `/docs` (no CDN dependencies)
- Spec served at `/openapi.json`
- `EndpointDoc` trait for summary, description, tags, operation ID
- `QueryParameters` trait for typed query params in the spec
- `ToSchema` impls for common types + `schemars` bridge (feature-gated)

### Axum Interoperability (feature `axum-interop`)
- `Server::into_axum_router()` — embed wayward in Axum apps
- `Server::with_axum_fallback()` — embed Axum routes in wayward
- Bidirectional body type conversion

### WebSocket Support (feature `ws`)
- `WebSocketUpgrade` extractor for HTTP upgrade handshake
- `on_upgrade()` callback with tokio-tungstenite `WebSocketStream`

### Developer Experience
- `#[diagnostic::on_unimplemented]` on key traits for clear compile errors
- Comprehensive trybuild test suite (pass + fail cases)
- Criterion benchmarks comparing wayward vs Axum routing
- CI pipeline: test, clippy, fmt, docs
