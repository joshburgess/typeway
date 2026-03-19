# Remaining Priorities

The framework is feature-complete for v0.1. Everything below is post-v0.1 work, organized by category.

---

## Research Features (from DESIGN.md §14)

These push typeway beyond what Servant offers. Each is a significant design effort.

### Middleware as Type-Level Effects

Encode middleware requirements in the API type. A `Protected<AuthUser, E>` endpoint that's missing an auth middleware layer should be a compile error, not a runtime 500. The `Protected` wrapper exists today but only enforces that the handler accepts an `AuthUser` argument — it doesn't verify the corresponding middleware is actually applied via `.layer()`. The goal is a `Requires<AuthLayer>` bound on the endpoint type that the `Server` builder discharges at construction time.

### Session-Typed WebSocket Routes

Encode WebSocket message protocols as session types. The type system enforces message ordering: if the protocol says "server sends Greeting, client sends Auth, server sends Ok", sending out of order is a compile error. Ownership-based enforcement — after sending `Greeting`, the handle transitions to a type that can only receive `Auth`. This is novel; Servant doesn't have it.

### Content Negotiation Coproducts

A response type like `OneOf<Json<User>, Xml<User>, Html<UserPage>>` that automatically negotiates based on `Accept` headers. The OpenAPI spec would list all representations. Fits naturally into the existing `EndpointToOperation` trait system.

### Type-Level API Versioning

`Extends<V1, Changes>` for expressing API evolution with compile-time backward compatibility checks. V2 is a delta on V1 — added endpoints, changed response types — and the compiler verifies that V1 clients can still call V2 servers for unchanged endpoints.

---

## Practical Improvements

### Better Client Ergonomics

The client works but is minimal. Improvements:
- Generated method names: `.get_user(42)` instead of `.call::<Endpoint>((42,))`
- Cookie/session persistence across requests
- Request/response interceptors (logging, auth token injection)
- Streaming response body support
- Builder pattern for complex requests (custom headers, query params)

### OpenAPI Enhancements

- Response examples via a trait (`ExampleValue`)
- Request body examples
- Security scheme declarations from `Protected<Auth, E>` wrappers
- Automatic tag grouping by path prefix
- Markdown descriptions from doc comments on handler functions

### gRPC / Tonic Interop

Same idea as the Axum interop but for Tonic. Both are Tower-based, so `Server::into_tonic_router()` should be feasible. Would allow a single service to serve both REST (typeway) and gRPC (tonic) endpoints with shared middleware.

### Publish to crates.io

All crates are at 0.1.0 but not published. Steps:
- Final review of public API surface
- Ensure all doc tests pass (there's a pre-existing doctest failure in `typeway/src/lib.rs`)
- Add `license`, `description`, `repository`, `keywords`, `categories` to each Cargo.toml
- Publish in dependency order: core → macros → server → client → openapi → facade
- Create a GitHub release with changelog

---

## Migration Tool

### Phase 4 Polish (remaining)

- Interactive mode (`--interactive`) for ambiguous cases
- VSCode extension integration (convert selected code)
- `--partial` flag to convert only specific routes
- Detect and handle `axum::Router::merge()` across functions

---

## Infrastructure

### Benchmark Regression Gating

The last unchecked item from PRODUCTION-HARDENING.md. Criterion benchmarks exist but no automated baseline comparison in CI. Options: `bencher.dev`, `github-action-benchmark`, or custom artifact diff.

### Pre-existing Issues to Clean Up

- Doctest failure in `typeway/src/lib.rs` (the `?` operator on `Box<dyn Error>`)
- 5 rustdoc warnings (broken intra-doc links in `typeway-server`)
