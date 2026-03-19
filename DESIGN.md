# Typeway Design Document

This document is the architectural contract for the Typeway web framework.
All implementation decisions in subsequent phases must be consistent with
the designs described here. Changes require updating this document first.

Based on the Phase 0 spike (Steps 0.2–0.4) and the type theory / research
ideation review captured in SPIKE-REVIEW.md.

---

## 1. Core Principle: The API Is a Type

The entire HTTP API is described as a single Rust type. Servers, clients,
and OpenAPI schemas are all derived from that type. No runtime routing
tables are constructed by hand. No request/response types are specified
in multiple places.

```rust
type MyAPI = (
    Endpoint<Get, path!("users"), NoBody, Json<Vec<User>>>,
    Endpoint<Get, path!("users" / u32), NoBody, Json<User>>,
    Endpoint<Post, path!("users"), Json<CreateUser>, Json<User>>,
    Endpoint<Delete, path!("users" / u32), NoBody, StatusCode>,
);
```

This type is the single source of truth for:
- Server routing and handler type checking
- Client method generation
- OpenAPI 3.x specification derivation
- Compile-time completeness verification (all routes have handlers)

## 2. Path Encoding: HList with Marker-Type Literals

### HList Structure

Paths are encoded as heterogeneous lists (HList). This enables recursive
trait impls that naturally mirror the structure of URL path matching.

```rust
pub struct HNil;
pub struct HCons<H, T>(PhantomData<(H, T)>);
```

A path like `/users/:id/posts` is:

```rust
HCons<Lit<users>, HCons<Capture<u32>, HCons<Lit<posts>, HNil>>>
```

### Why HList, Not Tuples

Paths are inherently recursive: match one segment, then recurse on the
remainder. This is a catamorphism — the natural recursion scheme for
inductive data. HList supports this with `O(n)` trait impls via structural
recursion. Flat tuples would require a combinatorial explosion of impls
for every possible sequence of literal/capture segment combinations.

**Rule: HList for paths, flat tuples for API route collections.**

### Literal Segments: Marker Types + LitSegment Trait

Const generic `&'static str` is not stable in Rust. We encode literal
path segments as zero-sized marker types implementing a `LitSegment` trait:

```rust
pub trait LitSegment {
    const VALUE: &'static str;
}
```

Each unique string literal generates a marker type in a private module:

```rust
mod __typeway_lit {
    pub struct users;
    impl super::LitSegment for users {
        const VALUE: &'static str = "users";
    }
}
```

Users never write these. The `path!` proc-macro generates them from string
literals:

```rust
path!("users" / u32 / "posts")
// expands to:
HCons<Lit<__typeway_lit::users>, HCons<Capture<u32>, HCons<Lit<__typeway_lit::posts>, HNil>>>
```

### Capture Segments

```rust
pub struct Capture<T>(PhantomData<T>);
```

`T` must implement `FromStr` for runtime parsing. The type appears in
the handler's extracted arguments.

### Capture Tuple Extraction

The `PathSpec` trait computes the tuple of captured types from a path HList:

```rust
pub trait PathSpec {
    type Captures;
}
```

Implemented via structural recursion:
- `HNil` → `Captures = ()`
- `HCons<Lit<S>, T>` → `Captures = T::Captures` (literals capture nothing)
- `HCons<Capture<U>, T>` → `Captures = Prepend<U, T::Captures>`

The `Prepend<T>` helper trait conses a type onto a tuple. Implemented via
`macro_rules!` for arities 0–8. This is `O(n)` constraint solving per
route, where n is the number of captures (capped at 8).

### CaptureRest (Wildcard Tail)

```rust
pub struct CaptureRest;
```

Matches all remaining path segments. `HCons<CaptureRest, HNil>` captures
a `Vec<String>` of the remaining segments. Must appear only at the tail.

## 3. Method Types

HTTP methods are zero-sized types implementing `HttpMethod`:

```rust
pub struct Get;
pub struct Post;
pub struct Put;
pub struct Delete;
pub struct Patch;
pub struct Head;
pub struct Options;

pub trait HttpMethod {
    const METHOD: http::Method;
}
```

Different method types are distinct Rust types. A `Route<Get, ...>` and
`Route<Post, ...>` with the same path cannot be confused.

## 4. Endpoint Type (API Specification Layer)

The `Endpoint` type describes a single HTTP endpoint at the API level.
This is the **specification** — it describes the HTTP interface for
OpenAPI generation and client derivation.

```rust
pub struct Endpoint<M, P, Req, Res> {
    _marker: PhantomData<(M, P, Req, Res)>,
}
```

Where:
- `M: HttpMethod` — the HTTP method
- `P: PathSpec` — the path HList
- `Req` — request body type (`NoBody` for bodyless methods)
- `Res` — the **declared** response type (the happy-path type for OpenAPI/clients)

Convenience aliases:

```rust
pub type GetEndpoint<P, Res>       = Endpoint<Get,    P, NoBody, Res>;
pub type PostEndpoint<P, Req, Res> = Endpoint<Post,   P, Req,    Res>;
pub type PutEndpoint<P, Req, Res>  = Endpoint<Put,    P, Req,    Res>;
pub type DeleteEndpoint<P, Res>    = Endpoint<Delete, P, NoBody, Res>;
pub type PatchEndpoint<P, Req, Res>= Endpoint<Patch,  P, Req,    Res>;
```

**Important distinction**: `Res` in `Endpoint` is the type that appears
in the OpenAPI spec and the generated client's return type. Handlers are
free to return `Result<Res, AppError>` or any `impl IntoResponse` — the
`CompatibleWith<Res>` trait (Section 6) bridges the gap.

`NoBody` is a unit type indicating no request body:

```rust
pub struct NoBody;
```

## 5. API Type: Tuple of Endpoints

An API is a flat tuple of `Endpoint` types:

```rust
pub trait ApiSpec {}
```

Implemented for:
- Every `Endpoint<M, P, Req, Res>`
- Every tuple of `ApiSpec` implementors, up to arity 16

Generated via `macro_rules!` — flat impls, no recursive trait chains.

```rust
type MyAPI = (
    GetEndpoint<path!("users"), Json<Vec<User>>>,
    GetEndpoint<path!("users" / u32), Json<User>>,
    PostEndpoint<path!("users"), Json<CreateUser>, Json<User>>,
    DeleteEndpoint<path!("users" / u32), StatusCode>,
);
```

For APIs with more than 16 routes, nest tuples:

```rust
type LargeAPI = (
    (Route1, Route2, ..., Route16),
    (Route17, Route18, ...),
);
```

`ApiSpec` is implemented for nested tuples via recursive impls on
the outer tuple.

## 6. Handler System: Extractor-Based Dispatch

### Design Decision: No H1/H2 Adapter Newtypes

The spike used `H1<F>`, `H2<F>` newtype wrappers to disambiguate handler
arities. This is replaced by the **extractor pattern**, proven by Axum to
work on stable Rust.

### Extractors

Each handler argument is an **extractor** — a type that knows how to
extract itself from the incoming request:

```rust
pub trait FromRequestParts: Sized {
    type Error: IntoResponse;
    async fn from_request_parts(parts: &mut Parts) -> Result<Self, Self::Error>;
}

pub trait FromRequest: Sized {
    type Error: IntoResponse;
    async fn from_request(req: Request<Body>) -> Result<Self, Self::Error>;
}
```

Built-in extractors:
- `Path<P: PathSpec>` — extracts `P::Captures` from URL segments
- `Json<T: DeserializeOwned>` — parses JSON request body
- `Query<T: DeserializeOwned>` — parses query string
- `State<T: Clone + Send + Sync>` — extracts shared application state
- `HeaderMap` — clones request headers
- `Bytes` — raw body bytes
- `String` — body as UTF-8

### Handler Trait

```rust
pub trait Handler<Args>: Clone + Send + 'static {
    type Future: Future<Output = Response> + Send;
    fn call(self, req: Request<Body>) -> Self::Future;
}
```

Implemented for async functions of arities 0–16 via `macro_rules!`:

```rust
// Arity 0
impl<F, Fut, Res> Handler<()> for F
where
    F: FnOnce() -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Res> + Send,
    Res: IntoResponse,
{ ... }

// Arity 1
impl<F, Fut, Res, T1> Handler<(T1,)> for F
where
    F: FnOnce(T1) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Res> + Send,
    Res: IntoResponse,
    T1: FromRequestParts,
{ ... }

// Arity 2
impl<F, Fut, Res, T1, T2> Handler<(T1, T2)> for F
where
    F: FnOnce(T1, T2) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Res> + Send,
    Res: IntoResponse,
    T1: FromRequestParts,
    T2: FromRequestParts,  // last non-body arg, OR FromRequest for body
{ ... }
```

Each arity is a different `Args` type parameter, so no overlapping impls.
The last argument may implement `FromRequest` (consuming the body) instead
of `FromRequestParts`.

### Example Handler

```rust
async fn get_user(
    Path(id): Path<path!("users" / u32)>,
    State(db): State<DbPool>,
) -> Result<Json<User>, AppError> {
    let user = db.find_user(id).await?;
    Ok(Json(user))
}
```

### Compatible Trait: Bridging Handlers to Endpoints

The `Compatible` trait verifies that a handler's extractors are consistent
with the endpoint specification:

```rust
pub trait Compatible<E: EndpointSpec> {}
```

Implementations verify:
- The handler's `Path<P>` extractor matches the endpoint's path type
- If the endpoint declares a request body `Req`, the handler must have
  a `Json<Req>` (or other body) extractor
- The handler's return type is `IntoResponse` and satisfies
  `CompatibleWith<E::Res>` (i.e., the happy-path type matches the
  declared response)

```rust
pub trait CompatibleWith<Declared> {}

// Json<T> is compatible with Json<T>
impl<T> CompatibleWith<Json<T>> for Json<T> {}

// Result<T, E> is compatible with T if T is compatible and E: IntoResponse
impl<T, E, D> CompatibleWith<D> for Result<T, E>
where
    T: CompatibleWith<D>,
    E: IntoResponse,
{}
```

### Serves Trait: API Completeness Check

```rust
pub trait Serves<A: ApiSpec> {}
```

Implemented for handler tuples that cover every endpoint in the API:

```rust
impl<H1, H2, H3, E1, E2, E3> Serves<(E1, E2, E3)> for (H1, H2, H3)
where
    H1: Handler<...> + Compatible<E1>,
    H2: Handler<...> + Compatible<E2>,
    H3: Handler<...> + Compatible<E3>,
{}
```

Generated via `macro_rules!` for arities 1–16. If you declare an API with
3 endpoints and provide 2 handlers, it does not compile.

## 7. Response System

### IntoResponse

```rust
pub trait IntoResponse {
    fn into_response(self) -> Response<BoxBody>;
}
```

Implemented for:
- `&'static str`, `String` — text/plain
- `Json<T: Serialize>` — application/json
- `StatusCode` — empty body with status
- `(StatusCode, impl IntoResponse)` — status + body
- `Result<T: IntoResponse, E: IntoResponse>` — success or error response
- `Response<BoxBody>` — identity

### CompatibleWith

Links the handler's actual return type to the endpoint's declared `Res`
for OpenAPI and client generation purposes. See Section 6.

## 8. Runtime Path Matching

```rust
pub trait ExtractPath: PathSpec {
    fn extract(segments: &[&str]) -> Option<Self::Captures>;
    fn pattern() -> String;  // "/users/{id}/posts/{post_id}"
}
```

Implemented for all HCons/HNil combinations via recursive trait impls:
- `Lit<S>` matches `segments[0] == S::VALUE`
- `Capture<T>` parses `segments[0]` via `T::from_str()`
- `CaptureRest` matches all remaining segments
- `HNil` matches only if no segments remain

The `pattern()` method returns the OpenAPI-format path string for use
in both routing and spec generation.

## 9. Router

```rust
pub struct Router {
    routes: Vec<RouteEntry>,
}

struct RouteEntry {
    method: http::Method,
    pattern: String,
    handler: BoxedHandler,
}
```

The router is a flat linear scan. For typical API sizes (<100 routes),
this beats a trie or hash map. Route matching:

1. Filter by HTTP method
2. Try each pattern's `ExtractPath::extract()` against the request path
3. Call the first matching handler
4. Return 404 if no match, 405 if path matches but method doesn't

Handlers are type-erased (boxed) at the point of insertion into the
router. Each handler is individually monomorphized, then stored as a
`BoxedHandler`. This keeps `Server<A>` thin — the monomorphization cost
is per-handler, not per-API-type.

## 10. Server Builder

```rust
pub struct Server<A: ApiSpec> {
    router: Router,
    _api: PhantomData<A>,
}

impl<A: ApiSpec> Server<A> {
    pub fn new<H: Serves<A>>(handlers: H) -> Self;
    pub fn with_state<T: Clone + Send + Sync + 'static>(self, state: T) -> Self;
    pub fn layer<L: Layer<...>>(self, layer: L) -> Self;
    pub async fn serve(self, addr: SocketAddr) -> Result<(), Error>;
    pub fn into_router(self) -> Router;
}
```

`new()` requires `H: Serves<A>`, the compile-time completeness check.
`layer()` accepts any Tower `Layer`, providing full middleware compatibility.
`into_router()` enables embedding in an Axum app via `.nest()`.

## 11. Macro Layer

All macros are syntactic sugar. They desugar to exactly the types in
`typeway-core`. No macro introduces runtime behavior.

### `path!` Macro

```rust
path!()                          → HNil
path!("users")                   → HCons<Lit<__typeway_lit::users>, HNil>
path!("users" / u32)             → HCons<Lit<__typeway_lit::users>, HCons<Capture<u32>, HNil>>
path!("users" / u32 / "posts")   → HCons<Lit<...>, HCons<Capture<u32>, HCons<Lit<...>, HNil>>>
```

Implemented as a proc-macro. Generates `LitSegment` marker types in a
private `__typeway_lit` module scoped to avoid collisions (each
invocation's literals are deduplicated within the crate).

### Route Convenience Macros

```rust
get!("users" / u32, Json<User>)
// → GetEndpoint<path!("users" / u32), Json<User>>

post!("users", Json<CreateUser>, Json<User>)
// → PostEndpoint<path!("users"), Json<CreateUser>, Json<User>>
```

### `#[handler]` Attribute (Optional Ergonomics)

```rust
#[handler]
async fn get_user(Path(id): Path<path!("users" / u32)>) -> Json<User> {
    // ...
}
```

Validates: function is async, argument types implement `FromRequestParts`
or `FromRequest`, return type implements `IntoResponse`. Emits clear
compile errors on mismatch.

This is **optional** — handlers work without the attribute. The attribute
provides better error messages.

## 12. Client Derivation

The `Client<A>` type derives HTTP client methods from the API type:

```rust
pub struct Client<A: ApiSpec> {
    base_url: Url,
    inner: reqwest::Client,
    _api: PhantomData<A>,
}
```

For each endpoint in `A`, the client provides a method that:
- Substitutes captures into the URL pattern
- Sets the HTTP method
- Serializes the request body (if any) as JSON
- Deserializes the response as the endpoint's `Res` type
- Returns `Result<Res, ClientError>`

The method signatures are fully type-checked against the API type.
If the server's API type changes, the client fails to compile until
updated.

## 13. OpenAPI Derivation

```rust
pub trait ApiToSpec {
    fn to_spec() -> OpenApiSpec;
}

pub trait EndpointToOperation {
    fn path_pattern() -> String;
    fn method() -> http::Method;
    fn to_operation() -> Operation;
}
```

Implemented for all `Endpoint` types via trait bounds:
- `P: ExtractPath` provides path pattern and parameter names
- `Req: JsonSchema` provides request body schema
- `Res: JsonSchema` provides response schema

The spec is generated at program startup, not compile time — `schemars`
needs runtime reflection. But the *structure* (which endpoints exist,
their methods and paths) is known from the types.

## 14. Future Directions (Not Blocking v0.1)

### Middleware as Type-Level Effects

Encode middleware requirements in the API type. Handlers declare needed
effects (`Authed`, `RateLimited`), the server builder discharges them.
Missing middleware is a compile error. See SPIKE-REVIEW.md, Idea 1.

### Session-Typed WebSocket Routes

Encode WebSocket message protocols as session types. Ownership-based
enforcement of protocol compliance. See SPIKE-REVIEW.md, Idea 2.

### Content Negotiation Coproducts

Route response types as coproducts of representations (`Json`, `Xml`,
`Html`). Automatic `Accept` header negotiation. Fits naturally into the
OpenAPI phase. See SPIKE-REVIEW.md, Idea 3.

### API Versioning

Type-level `Extends<V1, Changes>` for expressing API evolution with
compile-time backward compatibility checks.

## 15. Constraints and Non-Goals

### Stable Rust Only

All code must compile on stable Rust (edition 2021). No nightly features.
No `unsafe` unless absolutely unavoidable (and then with extensive
justification and testing).

### No Runtime Reflection

Type information flows at compile time. The only runtime type operations
are `FromStr` for path capture parsing and `serde` for body
serialization. No `TypeId`, no `Any` downcasting in the hot path.

### Compile Time Budget

| Scenario | Target (cold build, release) |
|---|---|
| `typeway-core` alone | < 2s |
| `typeway-server` + `typeway-core` | < 8s |
| Example with 5 routes, no openapi | < 15s |
| Example with 5 routes, full features | < 25s |
| Example with 20 routes, full features | < 45s |

If exceeded, investigate before adding features.

### Dependency Policy

- Tokio ecosystem only (no async-std)
- Pin major versions, never use `*`
- OpenAPI (`schemars`) and client (`reqwest`) behind feature flags
- `typeway-core` has minimal dependencies (only `http` crate)

### Arity Caps

- Path captures: 8 (via `Prepend` impls)
- Handler arguments: 16 (via `Handler` arity impls)
- API routes per tuple: 16 (nest for more)

These are enforced by the `macro_rules!` impl generation and are
sufficient for all practical use cases.
