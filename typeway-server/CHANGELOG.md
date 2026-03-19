# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0

### added:

- **Compile-time handler verification**: `Serves<API>` ensures every endpoint has a handler
- **Handler trait**: blanket impls for async functions of arities 0–8, with `FromRequestParts` and `FromRequest` extraction
- **HTTP/1.1 + HTTP/2**: automatic protocol detection via `hyper_util::server::conn::auto`
- **TLS/HTTPS** (feature `tls`): `TlsConfig::from_pem()` + `Server::serve_tls()`
- **Extractors**: `Path<P>`, `State<T>`, `Query<T>`, `Json<T>`, `Extension<T>`, `Header<T>`, `Cookie<T>`, `CookieJar`, `HeaderMap`, `http::Method`, `http::Uri`, `Bytes`, `String`
- **Responses**: `IntoResponse` trait with impls for `&str`, `String`, `Json<T>`, `StatusCode`, `(StatusCode, T)`, `Result<T, E>`, `Bytes`, `Response<BoxBody>`
- **Streaming**: `body_from_stream()` and `sse_body()` for chunked/SSE responses
- **Body size limits**: configurable via `Server::max_body_size()`, default 2 MiB
- **Structured errors**: `JsonError` with convenience constructors and JSON serialization
- **Router**: method-indexed linear scan with first-segment prefix rejection
- **Route nesting**: `Server::nest("/prefix")` for path prefix grouping
- **Static file serving**: `Server::with_static_files("/static", dir)` with MIME detection and directory index
- **SPA fallback**: `Server::with_spa_fallback("index.html")` for client-side routing
- **Tower middleware**: `.layer()` supports any Tower layer (CORS, timeout, compression, tracing)
- **Request ID**: `RequestIdLayer` generates UUID v4 per request
- **Fallback**: `Server::with_fallback()` for Tower service fallback on unmatched routes
- **Graceful shutdown**: `Server::serve_with_shutdown(listener, signal)`
- **Multipart upload** (feature `multipart`): `Multipart` extractor wrapping `multer`
- **`LayeredServer`**: all config methods work after `.layer()` calls
- **Axum interop** (feature `axum-interop`): `Server::into_axum_router()` and `Server::with_axum_fallback()` for bidirectional embedding
- **WebSocket support** (feature `ws`): `WebSocketUpgrade` extractor with tokio-tungstenite
- **Panic safety**: `RouterService` catches handler panics via `catch_unwind`, returns 500
- **`SecureHeadersLayer`**: Tower layer setting `X-Content-Type-Options`, `X-Frame-Options`, `X-XSS-Protection`, `Referrer-Policy`, `Content-Security-Policy`, `Permissions-Policy` with builder pattern for HSTS, custom headers, and per-header overrides
- **Production docs**: `production` module documenting health checks, graceful shutdown, load balancer draining, recommended middleware stack, panic recovery
- **`#[diagnostic::on_unimplemented]`** on `Handler` trait for clear compile errors
