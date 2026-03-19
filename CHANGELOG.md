# Changelog

## 0.1.0 — Initial Release

### Core Framework
- **API-as-type**: define your entire HTTP API as a Rust type — a tuple of endpoint descriptors
- **Compile-time handler verification**: `Serves<API>` ensures every endpoint has a handler
- **Type-level path encoding**: HList-based path segments with `Lit<S>` and `Capture<T>`
- **Endpoint types**: `GetEndpoint`, `PostEndpoint`, `PutEndpoint`, `DeleteEndpoint`, `PatchEndpoint` with optional query parameter and error type
- **Typed error responses**: `Endpoint<..., Err = JsonError>` — error schemas appear in OpenAPI spec and client knows what to deserialize
- **API tuple support**: up to 20 endpoints per tuple

### Type-Level Endpoint Wrappers
- **`Protected<Auth, E>`** — compile-time auth enforcement; handler MUST accept Auth as first arg
- **`Validated<V, E>`** — request body validation before handler; returns 422 on failure
- **`Versioned<V, E>`** — API version routing (`/v1/users`, `/v2/users`)
- **`ContentType<C, E>`** — enforces request Content-Type; returns 415 on mismatch
- **`RateLimited<R, E>`** — rate limit declaration in the API type
- **`Strict<E>`** — handler return type must exactly match the declared Res type
- **`endpoint!` macro** — builder syntax for composing wrappers without manual nesting

### Server (`wayward-server`)
- **HTTP/1.1 + HTTP/2**: automatic protocol detection via `hyper_util::server::conn::auto`
- **TLS/HTTPS** (feature `tls`): `TlsConfig::from_pem()` + `Server::serve_tls()`
- **Structured logging**: `tracing` crate integration throughout
- **Extractors**: `Path<P>`, `State<T>`, `Query<T>`, `Json<T>`, `Extension<T>`, `Header<T>`, `Cookie<T>`, `CookieJar`, `HeaderMap`, `http::Method`, `http::Uri`, `Bytes`, `String`
- **Responses**: `IntoResponse` trait with impls for `&str`, `String`, `Json<T>`, `StatusCode`, `(StatusCode, T)`, `Result<T, E>`, `Bytes`, `Response<BoxBody>`
- **Streaming**: `body_from_stream()` and `sse_body()` for chunked/SSE responses
- **Body size limits**: configurable via `Server::max_body_size()`, default 2 MiB
- **Structured errors**: `JsonError` with convenience constructors (`bad_request`, `not_found`, `unauthorized`, etc.) and JSON serialization
- **Route nesting**: `Server::nest("/prefix")` for path prefix grouping
- **Static file serving**: `Server::with_static_files("/static", dir)` with MIME detection and directory index
- **SPA fallback**: `Server::with_spa_fallback("index.html")` for client-side routing
- **Tower middleware**: `.layer()` supports any Tower layer (CORS, timeout, compression, tracing)
- **Request ID**: `RequestIdLayer` generates UUID v4 per request, generic over body type
- **Fallback**: `Server::with_fallback()` for Tower service fallback on unmatched routes
- **Graceful shutdown**: `Server::serve_with_shutdown(listener, signal)`
- **Multipart upload** (feature `multipart`): `Multipart` extractor wraps `multer`
- **`LayeredServer` config**: all config methods (`with_state`, `nest`, `with_static_files`, etc.) work after `.layer()` calls

### Macros (`wayward-macros`)
- `wayward_path!`: ergonomic path type definitions (`wayward_path!(type P = "users" / u32)`)
- `wayward_api!`: inline API type definitions with method/path/body syntax
- `endpoint!`: builder macro for composing type-level wrappers
- `#[handler]`: validates handler functions at definition site
- `#[api_description]`: trait-based API definition with auto-generated endpoint types
- `bind!()`, `bind_auth!()`, `bind_strict!()`, `bind_validated!()`, `bind_content_type!()`: handler binding macros

### Client (`wayward-client`)
- Type-safe HTTP client derived from the same API types as the server
- `Client::call::<Endpoint>(args)` with compile-time verification
- Supports all HTTP methods, path captures, and request/response bodies

### OpenAPI (`wayward-openapi`)
- OpenAPI 3.1 spec generation from API types at startup
- Embedded docs UI at `/docs` (no CDN dependencies)
- Spec served at `/openapi.json`
- `EndpointDoc` trait for summary, description, tags, operation ID
- `ErrorResponses` trait — typed error schemas in the spec
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
- Criterion benchmarks measuring dispatch overhead and routing performance
- CI pipeline: test, clippy, fmt, docs
- RealWorld ("Wayward Word") example: 19-endpoint Medium clone with Elm frontend, PostgreSQL, JWT auth, Docker Compose

### Performance
- Type erasure overhead: ~878ns per dispatch (< 0.1% of real handler time)
- Extractors: ~300ns each
- Body collection: O(1) via reference-counted `Bytes`
- Router: method-indexed linear scan with first-segment prefix rejection
