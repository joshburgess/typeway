# Versioned API with Compile-Time Effect Enforcement

An API that evolves from V1 to V2 where:

- **The changelog is a type** — not a markdown file, not documentation comments
- **Middleware requirements are enforced at compile time** — forget the rate limiter and the code won't build

## The core idea

V1 is a plain API type:

```rust
type V1 = (
    GetEndpoint<UsersPath, Vec<UserV1>>,
    GetEndpoint<UserByIdPath, UserV1>,
);
```

V2 is defined as **typed changes** applied to V1:

```rust
type V2Changes = (
    Replaced<GetEndpoint<UserByIdPath, UserV1>, GetEndpoint<UserByIdPath, UserV2>>,
    Added<GetEndpoint<UserProfilePath, UserProfile>>,
    Added<Requires<RateLimitRequired, GetEndpoint<HealthPath, HealthCheck>>>,
    Deprecated<GetEndpoint<UsersPath, Vec<UserV1>>>,
);
```

The compiler knows:
- 2 endpoints were added
- 1 was replaced (UserV1 → UserV2)
- 1 was deprecated
- The health check requires `RateLimitRequired`

## Compile-time enforcement

The health check endpoint is wrapped in `Requires<RateLimitRequired, ...>`.
The `EffectfulServer` tracks which effects have been provided:

```rust
EffectfulServer::<V2>::new(handlers)
    .provide::<RateLimitRequired>()  // discharge the requirement
    .layer(rate_limit_middleware)
    .ready()                         // compiles only if ALL effects provided
    .serve(addr)
    .await?;
```

Remove the `.provide::<RateLimitRequired>()` line and the code **won't compile**.
The error message tells you exactly which effect is missing.

## Run

```bash
cargo run -p typeway-versioned-api
```

## Test

```bash
# V1 endpoints (still work, deprecated):
curl http://localhost:3000/users

# V2 endpoints:
curl http://localhost:3000/users/1           # now includes email
curl http://localhost:3000/users/1/profile   # new in V2
curl http://localhost:3000/health            # new, rate-limited
```

## What makes this special

1. **The changelog is queryable at compile time.** `V2Changes::ADDED`,
   `V2Changes::DEPRECATED`, etc. are `const` values derived from the type.

2. **Middleware requirements are per-endpoint.** The health check requires
   rate limiting, but user endpoints don't. The effect system tracks this
   at the type level — no runtime checks, no middleware ordering bugs.

3. **Deprecation is visible in the type system.** `Deprecated<E>` marks
   endpoints as deprecated in generated OpenAPI specs and documentation,
   but they continue to function.

4. **Breaking changes are explicit.** `Replaced<Old, New>` and `Removed<E>`
   force you to acknowledge what changed. No accidental breakage.

## Files

| File | Purpose |
|------|---------|
| `src/main.rs` | V1 → V2 evolution with effects |
