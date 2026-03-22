# Typeway Guides

Practical, standalone guides for building with typeway.

## Getting Started

| Guide | What you'll learn |
|-------|-------------------|
| [Proto-First Codegen](proto-first-codegen.md) | Generate Rust types from `.proto` files |
| [Adding gRPC to REST](adding-grpc-to-rest.md) | Serve REST and gRPC from the same handlers |
| [Server Streaming](server-streaming.md) | Real-time data feeds with backpressure |
| [Direct Handlers](direct-handlers.md) | Bypass extractors for maximum throughput |

## REST & gRPC

| Guide | What you'll learn |
|-------|-------------------|
| [Standard gRPC Clients](standard-grpc-clients.md) | Interop with grpcurl, Postman, Tonic |
| [Optimizing with BytesStr](optimizing-with-bytesstr.md) | Zero-copy string decode (54% faster) |
| [OpenAPI](openapi.md) | Auto-generate OpenAPI specs from your API type |

## Production Patterns

| Guide | What you'll learn |
|-------|-------------------|
| [Authentication](authentication.md) | Type-safe auth with `Protected<Auth, E>` |
| [Request Validation](request-validation.md) | Body validation with `Validated<V, E>` |
| [Error Handling](error-handling.md) | Typed errors, gRPC status codes, rich details |
| [Middleware & Effects](middleware-and-effects.md) | Compile-time middleware requirements |
| [Testing](testing.md) | Unit tests, integration tests, compile-time tests |

## Advanced

| Guide | What you'll learn |
|-------|-------------------|
| [WebSockets](websockets.md) | Session-typed WebSocket protocols |
| [File Serving](file-serving.md) | Static files, SPAs, and fallbacks |

## Examples

See [`examples/`](../examples/) for complete runnable applications:

- **[realworld](../examples/realworld/)** — Full REST API (RealWorld spec)
- **[chat](../examples/chat/)** — Session-typed WebSocket chat
- **[versioned-api](../examples/versioned-api/)** — Type-level API versioning
- **[iot-gateway](../examples/iot-gateway/)** — Dual-protocol REST + gRPC
- **[orderbook](../examples/orderbook/)** — High-performance gRPC (Rust-first + proto-first)
