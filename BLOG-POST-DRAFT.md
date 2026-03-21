# Introducing `typeway`: Applying Learnings from Haskell's Servant to the Rust Ecosystem

I built a web framework where the entire HTTP API is a Rust type. Not a runtime data structure. Not a configuration file. A type — checked by the compiler, erased before the binary ships, zero overhead at runtime.

The server, the client, and the OpenAPI spec are all projections of that one type. If one changes, the others refuse to compile until they catch up.

This is Typeway. It sits on top of Tokio, Tower, and Hyper — the same foundation as Axum — and it's the answer to a question I've been chewing on for a while: can Haskell's Servant work in Rust, on stable, without the compile-time apocalypse?

The answer is yes, with caveats. This post is about how.

---

## The Problem With Runtime Routing

Here's a standard Axum server:

```rust
let app = Router::new()
    .route("/users", get(list_users))
    .route("/users/:id", get(get_user))
    .route("/users", post(create_user));
```

This works. Millions of production services are built this way. But there are entire categories of bugs that this style can't prevent:

**Missing handlers.** If you add a route to your API spec but forget to register the handler, you get a 404 at runtime. No compile error. No test failure unless you wrote one.

**Server-client drift.** Your client code makes HTTP calls based on what the API *used to* look like. The server changed the response type from `User` to `UserResponse` three weeks ago. The client code still compiles, still runs — and silently deserializes the wrong fields.

**OpenAPI rot.** Your spec says the endpoint returns a `User` with an `email` field. The handler actually returns a `UserProfile` without one. The spec is a lie, and it will stay a lie until someone notices.

These aren't exotic failure modes. They're the default state of any API that lives long enough. The server, client, and documentation are three independent artifacts that must be manually kept in sync, and they always drift apart eventually.

What if they couldn't?

## The API Is a Type

In Typeway, you write this:

```rust
typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);

type API = (
    GetEndpoint<UsersPath, Json<Vec<User>>>,
    GetEndpoint<UserByIdPath, Json<User>>,
    PostEndpoint<UsersPath, Json<CreateUser>, Json<User>>,
    DeleteEndpoint<UserByIdPath, StatusCode>,
);
```

`API` is a type alias. It's a tuple of endpoint descriptors, each encoding an HTTP method, a path, a request body type, and a response type. There is no runtime representation — `Endpoint<M, P, Req, Res>` is a zero-sized `PhantomData` marker. It exists only for the compiler.

From this single type, you derive everything:

```rust
// Server — compiler verifies every endpoint has a handler
Server::<API>::new((
    bind!(list_users),
    bind!(get_user),
    bind!(create_user),
    bind!(delete_user),
)).serve(addr).await?;

// Client — type-safe calls using the same endpoint types
let user = client.call::<GetEndpoint<UserByIdPath, User>>((42u32,)).await?;

// OpenAPI — spec generated from the type, no annotations needed
Server::<API>::new(handlers).with_openapi("My API", "1.0").serve(addr).await?;
```

Change the `API` type, and the compiler tells you everywhere that needs updating. The server won't compile with a missing handler. The client won't compile with a mismatched response type. The OpenAPI spec can't drift because it's derived from the types, not maintained by hand.

This is the central idea from Haskell's [Servant](https://docs.servant.dev/en/stable/), brought to Rust. But bringing it to Rust required solving problems that Haskell doesn't have.

---

## How It Works: Type-Level Programming in Stable Rust

### Paths as Heterogeneous Lists

The first design decision was how to encode URL paths in the type system. A path like `/users/:id/posts` has structure: it's a sequence of literal segments and capture segments, and that structure matters. A handler for this path expects a `u32` argument (the captured `:id`), and the runtime path parser needs to know which segments are literals and which are captures.

I use heterogeneous lists (HLists):

```rust
pub struct HNil;                             // empty path
pub struct HCons<H, T>(PhantomData<(H, T)>); // head segment + rest of path

pub struct Lit<S: LitSegment>(PhantomData<S>);  // literal segment, e.g., "users"
pub struct Capture<T>(PhantomData<T>);           // captured segment parsed as T
```

So `/users/:id/posts` becomes:

```rust
HCons<Lit<users>, HCons<Capture<u32>, HCons<Lit<posts>, HNil>>>
```

Why HLists instead of flat tuples? Because paths are inherently recursive: match one segment, then recurse on the remainder. This is a catamorphism — a fold over an inductive data structure — and HLists are the natural encoding for it in Rust's trait system.

With tuples, I'd need a separate trait impl for every possible combination of literal and capture segments at every length. `(Lit, Capture, Lit)` is different from `(Capture, Lit, Capture)` is different from `(Lit, Lit, Capture)` — combinatorial explosion. With HLists, I need exactly three impls: one for `HNil`, one for `HCons<Lit<S>, T>`, and one for `HCons<Capture<U>, T>`. The recursion handles the rest.

### Type-Level Computation: The PathSpec Catamorphism

The `PathSpec` trait is where the real work happens. It computes, at compile time, the tuple of types that a handler must accept as captured path parameters:

```rust
pub trait PathSpec {
    type Captures;  // the tuple of captured types
}

impl PathSpec for HNil {
    type Captures = ();
}

impl<S: LitSegment, T: PathSpec> PathSpec for HCons<Lit<S>, T> {
    type Captures = T::Captures;  // literals capture nothing, recurse
}

impl<U, T: PathSpec> PathSpec for HCons<Capture<U>, T>
where
    T::Captures: Prepend<U>,
{
    type Captures = <T::Captures as Prepend<U>>::Output;  // prepend U to captures
}
```

For the path `HCons<Lit<users>, HCons<Capture<u32>, HCons<Lit<posts>, HCons<Capture<u64>, HNil>>>>`:

1. `HCons<Lit<users>, ...>` — literal, skip. Recurse on tail.
2. `HCons<Capture<u32>, ...>` — capture! Prepend `u32` to tail's captures. Recurse.
3. `HCons<Lit<posts>, ...>` — literal, skip. Recurse.
4. `HCons<Capture<u64>, HNil>` — capture! Prepend `u64` to `()` → `(u64,)`.
5. Back up: prepend `u32` to `(u64,)` → `(u32, u64)`.

The compiler resolves this chain at compile time. `Captures = (u32, u64)`. The handler for this path must accept a `(u32, u64)`. If it doesn't, the trait bound fails and you get a compile error.

The `Prepend<T>` trait is a type-level cons operation on tuples, implemented via `macro_rules!` for arities 0–8:

```rust
impl<T> Prepend<T> for () {
    type Output = (T,);
}
impl<T, A> Prepend<T> for (A,) {
    type Output = (T, A);
}
impl<T, A, B> Prepend<T> for (A, B) {
    type Output = (T, A, B);
}
// ... up to arity 8
```

This is the trick: use `macro_rules!` to generate flat, concrete impls instead of recursive trait resolution chains. The compiler sees `Prepend<u32> for (u64,)` and immediately finds the impl. No recursion, no constraint explosion. This is critical for keeping compile times sane — more on that later.

### The Literal Segment Problem

There's a wrinkle. I want to write `Lit<"users">` — a type parameterized by a string. But `&'static str` as a const generic parameter requires the unstable `adt_const_params` feature, which has been unstable since 2021 with no stabilization date in sight. Nightly is not an option for a production framework.

The workaround: each unique path literal becomes a zero-sized marker type implementing a `LitSegment` trait:

```rust
pub trait LitSegment {
    const VALUE: &'static str;
}

// Generated by the typeway_path! proc macro:
struct users;
impl LitSegment for users {
    const VALUE: &'static str = "users";
}
```

The `typeway_path!` macro hides this entirely:

```rust
typeway_path!(type UsersPath = "users" / u32 / "posts");
// Generates:
//   mod __wp_UsersPath {
//       pub struct __lit_users;
//       impl LitSegment for __lit_users { const VALUE: &'static str = "users"; }
//       pub struct __lit_posts;
//       impl LitSegment for __lit_posts { const VALUE: &'static str = "posts"; }
//   }
//   type UsersPath = HCons<Lit<__lit_users>, HCons<Capture<u32>, HCons<Lit<__lit_posts>, HNil>>>;
```

Is this elegant? Not particularly. The generated module names leak into compiler errors. But it works on stable Rust, it's zero-cost at runtime, and the macro means users never see the internals. When `adt_const_params` stabilizes, `Lit<"users">` becomes a drop-in replacement behind a feature flag. The architecture is ready; the language just needs to catch up.

### Compile-Time Handler Completeness

This is the flagship guarantee. You define an API with N endpoints, and the compiler ensures you provide exactly N handlers — one per endpoint, in order.

The mechanism is the `Serves` trait:

```rust
pub trait Serves<A: ApiSpec> {
    fn register(self, router: &mut Router);
}
```

Implemented for tuples of bound handlers matching tuples of endpoints:

```rust
impl<E0, E1, E2> Serves<(E0, E1, E2)> for (BoundHandler<E0>, BoundHandler<E1>, BoundHandler<E2>)
where E0: ApiSpec, E1: ApiSpec, E2: ApiSpec
{
    fn register(self, router: &mut Router) {
        self.0.register_into(router);
        self.1.register_into(router);
        self.2.register_into(router);
    }
}
```

This is generated by `macro_rules!` for arities 1–20. When you write:

```rust
Server::<API>::new((
    bind!(list_users),
    bind!(get_user),
    bind!(create_user),
))
```

The compiler checks: does `(BoundHandler<E0>, BoundHandler<E1>, BoundHandler<E2>)` implement `Serves<(E0, E1, E2)>`? Only if the tuple lengths match and each `BoundHandler<E>` was created from a function with the right argument types for that endpoint.

If you forget a handler:

```rust
Server::<API>::new((
    bind!(list_users),
    bind!(get_user),
    // missing: create_user
    // missing: delete_user
))
```

The compiler rejects it — the handler tuple has arity 2, but the API has arity 4. No `Serves` impl exists for that combination. The error message, improved with `#[diagnostic::on_unimplemented]`, tells you what went wrong.

### The Extractor Pattern: Stealing From Axum

For the handler function signatures, I adopted Axum's extractor pattern wholesale. Each handler argument is a type that knows how to extract itself from the request:

```rust
pub trait FromRequestParts: Sized {
    type Error: IntoResponse;
    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error>;
}
```

`Path<P>` extracts captured path segments. `State<T>` extracts shared application state. `Json<T>` deserializes the request body. Handlers are normal async functions:

```rust
async fn get_user(
    path: Path<UserByIdPath>,
    state: State<DbPool>,
) -> Result<Json<User>, JsonError> {
    let (id,) = path.0;
    let user = state.find_user(id).await?;
    Ok(Json(user))
}
```

The `Handler` trait is implemented for async functions of arities 0–8 via `macro_rules!`, with each argument constrained to `FromRequestParts` (or `FromRequest` for the last argument, which consumes the body). This is the same approach Axum uses, and it's proven to work well on stable Rust.

---

## Taming Compile Times: Lessons From Servant's Mistakes

Servant is notorious for slow compilation. A 50-endpoint Haskell API can take minutes to compile. The root cause is recursive type-class resolution: GHC solves deeply nested type families and instance chains at every use site, and the work grows exponentially with API size.

I treated this as a first-class constraint from day one. Every design decision was evaluated not just for correctness and ergonomics, but for compile-time cost. Here's how Typeway avoids the Servant trap:

**Flat impls instead of recursive resolution.** The `Serves`, `Handler`, `Prepend`, and `ApiSpec` traits are all implemented via `macro_rules!` generating concrete impls for arities 1–20. The compiler sees `Serves<(E0, E1, E2)> for (BH0, BH1, BH2)` and finds it in O(1). Servant's equivalent resolves recursively, with each level requiring the previous one.

**Type erasure at the router boundary.** Handlers are individually monomorphized, then boxed as `BoxedHandler = Box<dyn Fn(Parts, Bytes) -> Pin<Box<dyn Future>>>`. This means `Server<API>` doesn't carry the handler types — it holds a `Router` containing type-erased closures. The monomorphization cost is per-handler (each handler generates one specialized closure), not per-API (which would generate one massive dispatch function for all handlers combined).

**HLists only where needed.** Paths use HLists because their recursive structure demands it. But the API type, the handler tuple, and the extractor arguments all use flat tuples. This minimizes the number of recursive trait chains the compiler has to evaluate.

**Minimal core dependencies.** `typeway-core` depends only on the `http` crate. The heavy dependencies — `serde`, `hyper`, `tower`, `reqwest`, `schemars` — are isolated in their respective crates behind feature flags. A `--no-default-features` build compiles fast.

The result: an API with 5 routes compiles in under 15 seconds. An API with 20 routes compiles in under 45 seconds. Compile time scales linearly with route count, not exponentially.

---

## Interop: Not an Island

The decision that makes Typeway practical, rather than a research project, is full Tower/Axum interoperability.

Typeway's router implements `tower::Service`. This means every Tower middleware works out of the box:

```rust
Server::<API>::new(handlers)
    .layer(SecureHeadersLayer::new())
    .layer(TraceLayer::new_for_http())
    .layer(CorsLayer::permissive())
    .layer(TimeoutLayer::new(Duration::from_secs(30)))
    .layer(CompressionLayer::new())
    .serve(addr)
    .await?;
```

And because Axum is also built on Tower/Hyper, the interop is bidirectional. Embed Typeway inside an Axum app:

```rust
let typeway_api = Server::<API>::new(handlers);
let app = axum::Router::new()
    .nest("/api/v1", typeway_api.into_axum_router())
    .route("/health", get(|| async { "ok" }));
```

Or embed Axum routes inside Typeway:

```rust
Server::<API>::new(handlers)
    .with_axum_fallback(axum_router)
    .serve(addr)
    .await?;
```

This means you can adopt Typeway incrementally. Use it for the endpoints where compile-time correctness matters most — a payments API, an auth system, a service contract boundary — and keep everything else in Axum. No all-or-nothing commitment. No rewrite. One binary, one runtime, shared middleware.

---

## The Ecosystem Advantage: Why Tower/Hyper/Tokio Beats a Custom Stack

The decision to build on Tower, Hyper, and Tokio isn't just about reusing code. It's about inheriting a production-ready ecosystem that no custom stack — and no Haskell web framework — can match in breadth and battle-testing.

### Tower Middleware: Already Solved

Tower's `Service` trait is the universal interface for async request/response processing in Rust. By implementing it for Typeway's router, I get the entire tower-http crate for free: CORS, compression, tracing, timeouts, rate limiting, request IDs, content-type validation. These aren't toy implementations — they're used by thousands of production services, maintained by the Tokio team, and continuously fuzzed and benchmarked.

Haskell's Servant has no standard middleware story. WAI (Web Application Interface) exists, but it operates below Servant's type level. You can't express "this endpoint requires authentication" in the Servant type and have middleware enforce it. WAI middleware is applied to the entire application as an opaque transformation — there's no connection between the middleware and the API type. In Typeway, a custom extractor like `AuthUser` participates in the type system: a handler that takes `AuthUser` won't compile if the auth middleware hasn't been configured, and the OpenAPI spec automatically documents the endpoint as requiring authentication.

Writing middleware from scratch — correctly handling CORS preflight requests, content negotiation for compression, timeout cancellation, rate limit headers — is months of work and a permanent maintenance burden. Tower's ecosystem represents years of production hardening that I inherit for free.

### Hyper: Speed Without Compromise

Hyper is one of the fastest HTTP implementations in any language. Connection management, HTTP/1.1 and HTTP/2 protocol details, keep-alive, chunked transfer encoding, upgrade handshakes — all handled, exhaustively tested, and continuously fuzzed. Typeway adds only routing and extraction on top; the hot path of connection handling is pure Hyper.

Haskell's warp is a solid HTTP server, but it has known performance cliffs under high concurrency. When request rates spike, warp's thread scheduling interacts poorly with GHC's garbage collector, causing latency spikes that don't show up in microbenchmarks but hurt production p99s. Hyper, running on Tokio's work-stealing scheduler, doesn't have this problem — Rust has no GC, and Tokio's task scheduling is designed for sustained high throughput.

### Tokio: No Runtime Split

Every non-trivial async Rust application already depends on Tokio. Your database driver (sqlx, diesel-async, sea-orm), your Redis client (fred, redis-rs), your message queue consumer (lapin, rdkafka), your gRPC service (tonic) — all share one Tokio runtime with one set of configuration knobs. Typeway sits in that same runtime. No adapter layers, no dual-runtime problem, no `block_on` bridges between executors.

Haskell has genuinely better concurrency primitives — STM (Software Transactional Memory) and green threads are elegant and powerful. But the *web library* ecosystem is fragmented. Servant, warp, scotty, snap, yesod, IHP — each has its own approach to middleware, error handling, and deployment. They don't share middleware. They don't share extractors. A library written for warp doesn't work with servant-server. This fragmentation means every framework reinvents basic infrastructure, and none of them achieves the depth of tower-http.

### Axum Interop: The Unique Advantage

No other type-safe web framework offers bidirectional embedding with a mainstream framework. Servant can't embed a WAI application inside a Servant server and have the WAI routes participate in the type-level API description. Dropshot has no interop with Axum or Actix. Warp's filter system is self-contained.

Typeway's Tower-native architecture makes incremental adoption real, not theoretical:

```rust
// Nest a type-safe API group inside an existing Axum application
let typeway_api = Server::<PaymentsAPI>::new(handlers);
let app = axum::Router::new()
    .nest("/api/v1/payments", typeway_api.into_axum_router())
    .route("/health", get(|| async { "ok" }));
```

This means a team can adopt Typeway for one service boundary — a payment API, a permissions system, an inter-service contract — without touching the rest of the application. The Axum routes keep working. The Tower middleware is shared. It's the same binary, the same runtime, the same deployment. The cost of trying Typeway is near zero, because backing out is just removing a `.nest()` call.

### Where Haskell's Web Ecosystem Falls Short

I want to be specific here, because vague "Haskell is hard in production" claims aren't useful. These are concrete pain points I've encountered or seen reported consistently:

- **Streaming responses.** servant-conduit exists but integrates awkwardly with the Servant type. Streaming a large JSON array or an SSE event stream requires dropping out of the type-safe API into raw WAI. In Typeway, `body_from_stream` and `sse_body` are first-class, type-checked response types.
- **WebSockets.** servant-websockets is limited and maintained sporadically. WebSocket upgrade handling in Haskell's web ecosystem requires manual plumbing through the WAI layer. Typeway delegates to Hyper's upgrade mechanism — the same code path that Axum's WebSocket support uses.
- **File uploads.** servant-multipart has rough edges around large file handling and streaming. The ecosystem hasn't converged on a standard multipart parser the way Rust has with `multer` (used by Axum and available to Typeway).
- **TLS configuration.** Setting up HTTPS in a Servant application requires manually wiring TLS through warp or WAI's TLS adapters. Typeway has a `tls` feature flag that wraps tokio-rustls — one line of configuration.
- **Structured logging.** Haskell has `katip` and `monad-logger`, but integrating them with WAI middleware requires boilerplate. Tower-http's `TraceLayer` combined with the `tracing` crate gives structured, span-aware logging across the entire request lifecycle with one `.layer()` call.

None of this diminishes Haskell's strengths. Purity, parametricity, and STM are genuine advantages for reasoning about concurrent systems. But when it comes to shipping a web service with streaming, WebSockets, file uploads, TLS, structured logging, CORS, compression, and monitoring — the Rust/Tokio ecosystem is more complete and more cohesive than anything available in Haskell today.

---

## What Rust's Type System Can and Can't Do

Building Typeway taught me where Rust's type system is remarkably expressive and where it forces uncomfortable workarounds.

**What works beautifully:**

- *Trait-level computation.* The `PathSpec` catamorphism, the `Prepend` type-level cons, the `Serves` completeness check — these are elegant, zero-cost, and the compiler resolves them fully at compile time. Rust's associated types are powerful enough to encode genuinely interesting type-level programs.

- *PhantomData as a design tool.* Zero-sized types carrying type information that the compiler uses but the runtime ignores. `Endpoint<Get, UsersPath, NoBody, Json<User>>` takes zero bytes. The entire API type takes zero bytes. It's pure compile-time information.

- *`macro_rules!` as a metaprogramming escape hatch.* When recursive trait resolution would be too expensive, `macro_rules!` generates flat impls. When the type system can't express something directly, proc macros generate the boilerplate. It's not elegant, but it's effective and the generated code is inspectable.

- *`#[diagnostic::on_unimplemented]`.* This attribute transforms inscrutable trait bound errors into actionable messages. Without it, a missing handler produces pages of generic constraint failures. With it, you get: "the handler tuple does not match the API specification."

**What requires workarounds:**

- *Const generic strings.* The `adt_const_params` feature would eliminate the marker type machinery entirely. `Lit<"users">` instead of `Lit<__wp_UsersPath::__lit_users>`. Cleaner types, cleaner errors, no proc macro needed for path definitions. This is the single biggest ergonomic improvement waiting on the language.

- *Specialization.* I maintain separate handler traits for different patterns (plain handlers, auth-required handlers, strict-return-type handlers) because overlapping trait impls aren't allowed on stable. Specialization would unify these into one trait.

- *Negative trait bounds.* I experimented with a type-level builder pattern where you construct endpoint types incrementally, and the compiler ensures all required fields are set before use. This requires expressing "T does NOT implement trait Unset" — which Rust can't do. The `endpoint!` macro is the pragmatic alternative.

## Type System Design Choices: What I Used and What I Didn't

Typeway runs on stable Rust. Every type-level trick in the framework compiles without nightly. But that doesn't mean I used every stable feature available — and it doesn't mean there aren't unstable features I'm actively waiting on. The choices about what *not* to use were as deliberate as the choices about what to use.

### GATs: Stable, But Not Worth It Here

Generic Associated Types stabilized in Rust 1.65. They let you write associated types with their own generic parameters:

```rust
trait StreamingExtractor {
    type Body<'a>: AsyncRead + 'a;
    fn extract<'a>(req: &'a mut Request) -> Self::Body<'a>;
}
```

I chose not to use them for the core API. The patterns that drive Typeway — HList path recursion, flat tuple impls via macros, PhantomData markers — are simpler constructs that don't benefit from GATs. Adding GATs to the core traits would increase trait complexity, produce harder-to-read error messages, and raise compile-time costs from resolving GAT bounds, all without improving the user-facing API.

Where GATs *could* help: a future streaming extractor that borrows from the request instead of returning owned `Bytes`. Currently, `FromRequest` returns owned data. A GAT-based version could return data with a lifetime tied to the request. That's a potential enhancement for advanced use cases, not a reason to restructure the foundation.

The general principle: don't adopt a feature because it's technically available. Adopt it because it makes the framework simpler to use and easier to debug.

### Const Generic Integers: Stable, But Named Types Are Clearer

Integer const generics have been stable since Rust 1.51. They let you parameterize types by numbers:

```rust
struct RateLimited<const MAX: u32, const WINDOW_SECS: u64, E>(PhantomData<E>);
type Limited = RateLimited<100, 60, GetEndpoint<UsersPath, String>>;
```

I could use this for rate limiting, max body sizes, timeout durations. I mostly don't. The trait-based approach is more readable at the call site:

```rust
// Const generics: concise but opaque
type E = RateLimited<100, 60, GetEndpoint<...>>;

// Trait-based: more lines to define, but self-documenting
struct StandardRate;
impl RateLimit for StandardRate {
    const MAX_REQUESTS: u32 = 100;
    const WINDOW_SECS: u64 = 60;
}
type E = RateLimited<StandardRate, GetEndpoint<...>>;
```

With const generics, `100, 60` is anonymous — you can't tell what those numbers mean without context, and you can't reuse them across endpoints without repeating them. With a named type like `StandardRate`, the meaning is self-documenting and the configuration is defined once.

I may adopt integer const generics selectively for simple, well-understood constants where a named type would be overkill. But for the typical case, named trait impls win on readability.

### Three Unstable Features I'm Waiting On

**Const generic `&'static str` — the big one.** This is gated behind `adt_const_params`, unstable since 2021 with no stabilization timeline. It would let me write `Lit<"users">` instead of generating a marker type per literal string. Here's what the framework looks like with and without it:

```rust
// Today (marker types via proc macro):
typeway_path!(type UsersPath = "users" / u32);
// Generates hidden module with __lit_users struct implementing LitSegment

// With const generic strings (no macro needed):
type UsersPath = HCons<Lit<"users">, HCons<Capture<u32>, HNil>>;
```

This single feature would eliminate `typeway_path!` entirely, remove the hidden `__wp_*` modules that pollute `cargo doc` output, make two paths with the same literal automatically the same type, and simplify compiler error messages from `Lit<__wp_UsersPath::__lit_users>` to `Lit<"users">`. The `LitSegment` trait, all marker type generation, and the module-scoping machinery become unnecessary. It's the single biggest ergonomic improvement waiting on the language.

The architecture is ready for it. When `adt_const_params` stabilizes, `Lit<"string">` becomes a drop-in replacement behind a feature flag. The existing macro approach stays as a backward-compatible alternative.

**Specialization — unifying handler traits.** Currently, I maintain separate handler traits for different patterns: `Handler`, `AuthHandler`, `StrictHandler`. Overlapping trait impls aren't allowed on stable, so I use marker types (`WithBody<Parts, Body>`, `AuthWithBody<Parts, Body>`) to disambiguate. Specialization would collapse these into a single `Handler` trait where the more specific impl wins. It's been unstable since 2016 with known soundness issues. Impact if stabilized: moderate — fewer traits, fewer marker types, same user-facing API.

**Negative trait impls — enabling the type-level builder.** I experimented with a type-level builder pattern where you construct endpoint types incrementally and the compiler ensures all required fields are set before use. This requires expressing "T does NOT implement trait `Unset`" — which Rust can't do. Without negative impls, the `NotUnset` marker trait had to be manually implemented for every user type, making the pattern impractical. The `endpoint!` macro is the pragmatic alternative. If negative impls stabilize, `impl<T: !Unset> NotUnset for T {}` makes the builder pattern viable without manual impls.

### The Underlying Principle

Every decision above follows the same logic: prefer simpler constructs that produce better error messages over technically impressive ones that produce inscrutable errors. GATs are powerful but make trait errors harder to parse. Const generic integers are concise but make configurations anonymous. The framework's job is to catch mistakes at compile time *and tell you what went wrong in plain language*. If a feature makes the type system more expressive but the error messages worse, it's not worth it for a framework that developers interact with through those error messages daily.

---

## The Type-Theoretic Perspective

For the PL-curious: what Typeway is doing has a precise type-theoretic interpretation.

The HList path encoding is an inductive type at the kind level. `PathSpec` is a catamorphism (fold) over this inductive type, computing a product type (the capture tuple) from the structure. This is the same pattern as a fold over a list in Haskell: `foldr (\seg acc -> if isCapture seg then (typeOf seg, acc) else acc) () path`.

The `Handler<Args>` trait is an approximation of a Pi type — "for this specific route shape, the handler must have *this* function type." Rust can't express dependent function types directly, but trait-level computation achieves the same effect: the function's argument types are determined by the route type via trait resolution.

The `Serves<API>` trait is a type-level map: given a tuple of endpoint types, produce a tuple of handler types, and check that the user's handler tuple matches. This is essentially `HMap Handler endpoints = handlers`, verified by unification.

The whole system is a shallow embedding of an API description language into Rust's type system. "Shallow" because the API type has no operational semantics of its own — it's interpreted by different trait impls (server dispatch, client calls, OpenAPI generation) that each project the type-level description into runtime behavior.

---

## Beyond Servant: Four Features Haskell Doesn't Have

Typeway started as a Servant port, but four recent features push it past what Servant offers — or what Haskell can express naturally without experimental extensions.

### Session-Typed WebSockets

A WebSocket connection is an untyped message pipe. You can send any message at any time, and if you send them in the wrong order, you find out at runtime — maybe. Session types fix this by encoding the protocol as a type: what message to send, what message to expect next, and when the conversation ends.

Here's a protocol where the server sends a greeting, receives a name, sends a welcome, and closes:

```rust
use typeway::session::*;

type GreetProtocol = Send<String, Recv<String, Send<String, End>>>;
```

And the handler:

```rust
use typeway::typed_ws::TypedWebSocket;

async fn greet_handler(ws: TypedWebSocket<GreetProtocol>) {
    let ws = ws.send("Hello! What is your name?".into()).await.unwrap();
    let (name, ws) = ws.recv().await.unwrap();
    let ws = ws.send(format!("Welcome, {name}!")).await.unwrap();
    ws.close().await.unwrap();
}
```

Each `.send()` consumes the channel and returns it at the next state. Each `.recv()` consumes the channel and returns the message plus the channel at the next state. When the protocol reaches `End`, the only available operation is `.close()`. If you try to call `.recv()` when the protocol says `Send`, the code does not compile — `TypedWebSocket<Send<String, Next>>` doesn't have a `recv` method.

The trick is that Rust's ownership system enforces linearity for free. In session type theory, linearity means each channel must be used exactly once — you can't skip a step or reuse a previous state. In Haskell, enforcing this requires linear types (the `LinearTypes` GHC extension, still experimental and not widely adopted). In Rust, it falls out of the move semantics that every Rust programmer already uses. When `ws.send(msg)` takes `self` by value, the old `ws` is gone. There is no way to use the channel in the wrong state because the wrong state no longer exists.

The `Dual` trait computes the mirror protocol automatically: `Send` becomes `Recv`, `Recv` becomes `Send`, `Offer` becomes `Select`. A single protocol definition generates both sides, and the type system guarantees compatibility. Branching (`Offer<L, R>` / `Select<L, R>`) and recursive protocols (`Rec<Body>` / `Var`) are supported, covering real-world protocols like chat rooms where the server loops receiving messages and broadcasting responses.

Haskell has session type libraries (`session-types`, `sessions`), but they require linear types to enforce protocol adherence — and GHC's `LinearTypes` extension is still marked experimental with limited ecosystem support. Servant has no session-typed WebSocket story at all; `servant-websockets` provides raw message pipes. This is a case where Rust's ownership model gives it a genuine type-safety advantage over Haskell.

### Content Negotiation as a Type-Level Coproduct

HTTP content negotiation — the `Accept` header dance where the client says what formats it can handle and the server picks the best one — is straightforward in theory but tedious in practice. Most frameworks either hardcode JSON or make you write manual header parsing.

In Typeway, the handler's return type declares which formats are available:

```rust
use typeway::negotiate::*;

async fn get_user(accept: AcceptHeader) -> NegotiatedResponse<User, (JsonFormat, TextFormat)> {
    negotiated(User { id: 1, name: "Alice".into() }, accept)
}
```

`NegotiatedResponse<T, Formats>` is parameterized by a domain type `T` and a tuple of format markers. The `AcceptHeader` extractor pulls the header from the request. At response time, the framework parses the `Accept` value (including quality weights and wildcards), finds the best match among the declared formats, and serializes `T` using the corresponding `RenderAs<Format>` impl. Blanket impls cover `RenderAs<JsonFormat>` for any `T: Serialize` and `RenderAs<TextFormat>` for any `T: Display`, so most types work without extra code.

The format tuple is a type-level coproduct: it declares the set of possible representations, and adding a new format is a type-level change that propagates through the system. If you add `CsvFormat` to the tuple but forget to implement `RenderAs<CsvFormat>` for your type, the compiler tells you. The OpenAPI spec could enumerate all supported representations automatically from the same tuple.

### Type-Level API Versioning

API versioning is where type-safe frameworks traditionally fall down. You define V1, you define V2, and the relationship between them — which endpoints were added, which were removed, which changed their types — exists only in your head or in a changelog file. Nothing prevents you from accidentally dropping an endpoint that clients depend on.

Typeway encodes the relationship between versions as typed deltas:

```rust
use typeway::versioning::*;

type V1 = (
    GetEndpoint<UsersPath, Json<Vec<UserV1>>>,
    GetEndpoint<UserByIdPath, Json<UserV1>>,
    PostEndpoint<UsersPath, Json<CreateUser>, Json<UserV1>>,
);

type V2Changes = (
    Added<GetEndpoint<UserProfilePath, Json<Profile>>>,
    Replaced<
        GetEndpoint<UserByIdPath, Json<UserV1>>,
        GetEndpoint<UserByIdPath, Json<UserV2>>,
    >,
    Deprecated<PostEndpoint<UsersPath, Json<CreateUser>, Json<UserV1>>>,
);

type V2 = VersionedApi<V1, V2Changes, V2Resolved>;
```

V2 isn't defined from scratch — it's defined as V1 plus a set of typed changes. `Added`, `Removed`, `Replaced`, and `Deprecated` are change markers that carry the endpoint types as parameters. The `ApiChangelog` trait counts them at compile time: `V2Changes::ADDED == 1`, `V2Changes::REPLACED == 1`, `V2Changes::DEPRECATED == 1`.

The compile-time compatibility check uses an index witness technique for type-level set membership. `HasEndpoint<E, Idx>` asserts that an endpoint type `E` exists in an API tuple, where `Idx` is a type-level natural number (`Here`, `There<Here>`, `There<There<Here>>`, ...) that tells the compiler which tuple position to check. Each position has a distinct index type, so there's no coherence conflict — the compiler finds exactly one impl per endpoint-position pair.

```rust
assert_api_compatible!(
    (GetEndpoint<UsersPath, Json<Vec<UserV1>>>,
     PostEndpoint<UsersPath, Json<CreateUser>, Json<UserV1>>),
    V2Resolved
);
```

This fails to compile if either endpoint is missing from `V2Resolved`. The error points at the specific endpoint that's absent. Endpoints that were intentionally replaced or removed are simply omitted from the compatibility check — the change markers document the intent, and the check covers what must be preserved.

Nothing like this exists in Servant. Haskell's type system is powerful enough to express it, but nobody has built it — Servant APIs are versioned by maintaining separate type aliases with no typed relationship between them. Typeway makes API evolution a first-class concept in the type system, with machine-checkable backward compatibility guarantees.

### gRPC from the Same API Type

This is the feature I'm most proud of, because nothing else in any language does it.

Your REST handlers automatically become gRPC endpoints. The same type that drives the REST server, the type-safe client, and the OpenAPI spec now also generates Protocol Buffers service definitions, serves gRPC alongside REST on the same port, provides a type-safe gRPC client, exposes server reflection, runs a health check service, serves gRPC documentation, supports gRPC-Web for browser clients, and validates proto compatibility across versions. One API type, eight projections: REST server, REST client, OpenAPI spec + Swagger UI, gRPC server, gRPC client, `.proto` file, gRPC spec + docs page, and server reflection.

`#[derive(ToProtoType)]` eliminates hand-written message definitions entirely. The Rust struct is the source of truth, with field tags declared inline for stable wire format numbering:

```rust
/// A registered user.
#[derive(ToProtoType)]
struct User {
    /// The unique user identifier.
    #[proto(tag = 1)]
    id: u32,
    /// Display name.
    #[proto(tag = 2)]
    name: String,
    /// Account metadata.
    #[proto(tag = 3)]
    metadata: HashMap<String, String>,
}
```

Doc comments flow through to the proto output. `HashMap` and `BTreeMap` map to proto `map<K,V>` fields. Enums work too: simple enums become proto `enum` definitions, tagged enums with data become `oneof` fields. Request message bodies are flattened — fields are inlined into the request message rather than wrapped in a `body` field, so the proto API looks natural.

The compile-time safety extends to the gRPC layer. `.with_grpc()` requires the API type to satisfy the `GrpcReady` trait — a compile-time check that every request and response type in the API has a `ToProtoType` implementation. If any type is missing, you get a compile error at the `.with_grpc()` call, not a runtime panic when a gRPC request arrives. This is the same philosophy as `Serves<API>` for handler completeness, applied to the proto layer.

Serving gRPC alongside REST is still a one-liner on the server builder:

```rust
Server::<API>::new(handlers)
    .with_grpc("UserService", "users.v1")
    .with_grpc_docs()
    .serve(addr)
    .await?;
```

The gRPC dispatch shares handlers between REST and gRPC — there is no duplication. A single handler implementation serves both protocols, sharing the same Tower middleware stack and Tokio runtime. The native dispatch uses HashMap lookup in `NativeMultiplexer` for direct method routing, handles gRPC framing (length-prefix encoding) with real HTTP/2 trailers for `grpc-status`, and propagates deadlines (the `grpc-timeout` header becomes a Tower timeout) transparently. Streaming uses real `tokio::sync::mpsc` channels with backpressure, not collect-and-split.

For encoding performance, `#[derive(TypewayCodec)]` generates compile-time specialized protobuf encoders that are 15-30% faster than prost on decode and 20-26% faster on roundtrip (benchmarked head-to-head with `#[derive(prost::Message)]` using Criterion). The speedup comes from compile-time field layout knowledge — tag numbers, wire types, and buffer sizes are constants, not runtime values. `BinaryCodec` provides standard protobuf interop (`application/grpc`) for clients that expect vanilla gRPC, while the default JSON codec (`application/grpc+json`) shares serialization with the REST path. `GrpcClient` is codec-aware and selects the right encoding automatically.

Streaming is supported across all three gRPC patterns. `ServerStream<E>` splits JSON arrays into per-element gRPC frames for server-streaming RPCs. `ClientStream<E>` handles client-streaming. `BidirectionalStream<E>` handles full-duplex streaming. All three generate the correct `stream` annotations in the `.proto` output.

Two macros generate type-safe gRPC clients. `grpc_client!` gives manual control over method names, while `auto_grpc_client!` derives them automatically from the API type:

```rust
auto_grpc_client! {
    pub struct UserServiceClient;
    api = UsersAPI;
    service = "UserService";
    package = "users.v1";
}
```

Both macros include a `GrpcReady` compile-time assertion. Client interceptors are configurable via `GrpcClientConfig` for metadata injection and timeouts.

Change the API type, and both the REST client and the gRPC client refuse to compile until they're updated. Server reflection means `grpcurl -plaintext localhost:3000 list` discovers services at runtime without needing a `.proto` file on disk. `.with_grpc_docs()` serves `/grpc-spec` (a structured JSON spec) and `/grpc-docs` (an HTML documentation page) — the gRPC equivalent of OpenAPI + Swagger UI. `GrpcWebLayer` handles browser clients that can't do HTTP/2 gRPC natively. The health check service handles graceful shutdown. The `IntoGrpcStatus` trait maps handler error types to gRPC status codes consistently across both protocols.

On the tooling side, `validate_proto()` checks generated proto files for validity, and `diff_protos()` compares two proto files and reports breaking vs. compatible changes — suitable for CI pipelines via the `typeway-grpc diff` CLI. For the reverse direction, `typeway-grpc api-from-proto` converts an existing `.proto` file into Typeway API types, and `typeway-grpc spec-from-proto` generates documentation from any proto file.

The contrast with Haskell is stark. Servant has no gRPC story at all. The Haskell gRPC ecosystem (`grpc-haskell`, `proto-lens`) is completely separate from Servant — different type hierarchies, different code generation pipelines, different middleware stacks. If you want both REST and gRPC in a Haskell service, you maintain two independent API definitions with no shared types, no shared handlers, no unified serving. Typeway unifies REST and gRPC under one type, one set of handlers, and one middleware stack. I'm not aware of any other web framework in any language that derives both REST and gRPC from a single type-level API definition.

This is a fundamentally different approach from Tonic. Tonic has years of production use and a large ecosystem. Typeway's gRPC is new and experimental. We think the type-level approach is better for projects already using Typeway, but we'd recommend Tonic for standalone gRPC services where ecosystem maturity matters.

---

## See It in Action

If you want to see Typeway used in anger rather than in blog post snippets, the repository includes a full [RealWorld example app](https://github.com/joshburgess/typeway/tree/main/examples/realworld) — a Medium clone implementing the [Conduit spec](https://github.com/gothinkster/realworld). It has 19+ endpoints, PostgreSQL via sqlx, JWT authentication, an Elm frontend, and a Docker Compose setup for local development. It's the best way to judge whether the framework scales to a real application.

For teams already on Axum, the `typeway-migrate` CLI tool provides zero-friction migration analysis. Run `typeway-migrate check` against your existing Axum project to get a report of which routes can be converted to type-safe endpoints. Run `typeway-migrate axum-to-typeway --dry-run` to see the generated Typeway types without modifying any files. The tool is new, but it's the fastest way to evaluate whether Typeway fits your codebase.

To try it yourself: `cargo add typeway --features full`.

---

## What You Get

If you're building APIs in Rust and you've ever been bitten by a missing route, a drifted client, or an out-of-date OpenAPI spec, Typeway eliminates those problems at the type level.

If you're interested in type-level programming in Rust, Typeway is a real-world case study in how far you can push stable Rust's trait system — HList catamorphisms, type-level computation, macro-generated flat impls, compile-time completeness proofs — all without nightly, all without `unsafe`.

If you're using Axum and you want stronger guarantees for part of your API, Typeway integrates directly. No migration. No separate service. Nest a type-safe API group alongside your existing routes and get compile-time verification where it matters most.

The API is the type. The rest follows.

---

*Typeway is open source at [github.com/joshburgess/typeway](https://github.com/joshburgess/typeway). It requires stable Rust 1.80+.*
