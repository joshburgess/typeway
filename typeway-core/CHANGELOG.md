# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0

### added:

- **API-as-type**: define an entire HTTP API as a Rust type, a tuple of endpoint descriptors
- **Type-level path encoding**: HList-based path segments with `Lit<S>` and `Capture<T>`
- **`PathSpec` trait**: computes captured types as associated type `Captures`
- **`ExtractPath` trait**: runtime path parsing with `FromStr`-based segment extraction
- **HTTP method types**: `Get`, `Post`, `Put`, `Delete`, `Patch`, `Head`, `Options` with `HttpMethod` trait
- **Endpoint types**: `GetEndpoint`, `PostEndpoint`, `PutEndpoint`, `DeleteEndpoint`, `PatchEndpoint` with optional query parameter and error type
- **`ApiSpec` trait**: marker trait for tuples of endpoints, implemented up to arity 25
- **Typed error responses**: `Endpoint<..., Err = JsonError>` encodes error schemas in the API type
- **Type-level endpoint wrappers**:
  - `Protected<Auth, E>`, compile-time auth enforcement
  - `Validated<V, E>`, request body validation
  - `Versioned<V, E>`. API version routing
  - `ContentType<C, E>`. Content-Type enforcement
  - `RateLimited<R, E>`, rate limit declaration
  - `Strict<E>`, exact return type matching
- **Tuple prepend**: `Prepend<T>` helper trait for arities 0–8
