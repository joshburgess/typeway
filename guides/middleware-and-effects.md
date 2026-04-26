# Middleware & the Effect System

Typeway has two middleware mechanisms:

1. **Tower layers**, standard `tower_http` middleware (CORS, timeouts, tracing)
2. **Type-level effects**, compile-time enforcement that required middleware is applied

## Tower middleware

Apply middleware with `.layer()`:

```rust
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;
use std::time::Duration;

Server::<API>::new(handlers)
    .layer(CorsLayer::permissive())
    .layer(TimeoutLayer::new(Duration::from_secs(30)))
    .serve(addr)
    .await?;
```

Common layers from `tower-http` (re-exported via `typeway::tower_http`):

| Layer | Purpose |
|-------|---------|
| `CorsLayer` | Cross-origin resource sharing |
| `TimeoutLayer` | Request timeout |
| `TraceLayer` | Request/response tracing |
| `CompressionLayer` | Response compression (gzip, deflate) |
| `SetResponseHeaderLayer` | Add response headers |
| `PropagateHeaderLayer` | Forward headers (request IDs) |

## The effect system

Tower layers are applied at runtime, the compiler can't verify that
required middleware is present. The effect system adds compile-time
enforcement.

### Declare requirements

Use `Requires<Effect, Endpoint>` in your API type:

```rust
use typeway_core::effects::*;

type API = (
    // This endpoint requires auth middleware
    Requires<AuthRequired, GetEndpoint<UsersPath, Vec<User>>>,

    // This endpoint requires CORS
    Requires<CorsRequired, GetEndpoint<PublicPath, PublicData>>,

    // This endpoint requires both
    Requires<AuthRequired,
        Requires<CorsRequired,
            PostEndpoint<UsersPath, CreateUser, User>
        >
    >,

    // This endpoint has no requirements
    GetEndpoint<HealthPath, String>,
);
```

### Provide effects

Use `EffectfulServer` and `.provide::<E>()` to discharge requirements:

```rust
use typeway_server::EffectfulServer;

EffectfulServer::<API>::new(handlers)
    .provide::<AuthRequired>()   // "I promise auth middleware is applied"
    .layer(auth_middleware)       // apply the actual middleware
    .provide::<CorsRequired>()
    .layer(CorsLayer::permissive())
    .ready()                     // compile-time check: all effects provided
    .serve(addr)
    .await?;
```

If you forget to provide a required effect, the code **won't compile**:

```rust
// ERROR: CorsRequired not provided
EffectfulServer::<API>::new(handlers)
    .provide::<AuthRequired>()
    .ready()  // ← compile error here
    .serve(addr)
    .await?;
```

### Built-in effect types

| Effect | Meaning |
|--------|---------|
| `AuthRequired` | Authentication middleware must be applied |
| `CorsRequired` | CORS headers must be configured |
| `RateLimitRequired` | Rate limiting must be applied |
| `TracingRequired` | Request tracing must be enabled |

### Custom effects

Define your own:

```rust
use typeway_core::effects::Effect;

struct AuditLogRequired;
impl Effect for AuditLogRequired {}

type AuditedAPI = (
    Requires<AuditLogRequired, DeleteEndpoint<UserByIdPath, ()>>,
);

EffectfulServer::<AuditedAPI>::new(handlers)
    .provide::<AuditLogRequired>()
    .layer(audit_log_layer)
    .ready()
    .serve(addr)
    .await?;
```

## When to use effects vs plain middleware

**Use `.layer()` alone** when middleware is global and you don't need
compile-time enforcement (e.g., compression, tracing).

**Use effects** when forgetting middleware would be a bug (e.g., auth
on admin endpoints, CORS on public APIs). The compiler catches mistakes
that code review might miss.
