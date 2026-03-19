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

---

## The Type-Theoretic Perspective

For the PL-curious: what Typeway is doing has a precise type-theoretic interpretation.

The HList path encoding is an inductive type at the kind level. `PathSpec` is a catamorphism (fold) over this inductive type, computing a product type (the capture tuple) from the structure. This is the same pattern as a fold over a list in Haskell: `foldr (\seg acc -> if isCapture seg then (typeOf seg, acc) else acc) () path`.

The `Handler<Args>` trait is an approximation of a Pi type — "for this specific route shape, the handler must have *this* function type." Rust can't express dependent function types directly, but trait-level computation achieves the same effect: the function's argument types are determined by the route type via trait resolution.

The `Serves<API>` trait is a type-level map: given a tuple of endpoint types, produce a tuple of handler types, and check that the user's handler tuple matches. This is essentially `HMap Handler endpoints = handlers`, verified by unification.

The whole system is a shallow embedding of an API description language into Rust's type system. "Shallow" because the API type has no operational semantics of its own — it's interpreted by different trait impls (server dispatch, client calls, OpenAPI generation) that each project the type-level description into runtime behavior.

---

## What You Get

If you're building APIs in Rust and you've ever been bitten by a missing route, a drifted client, or an out-of-date OpenAPI spec, Typeway eliminates those problems at the type level.

If you're interested in type-level programming in Rust, Typeway is a real-world case study in how far you can push stable Rust's trait system — HList catamorphisms, type-level computation, macro-generated flat impls, compile-time completeness proofs — all without nightly, all without `unsafe`.

If you're using Axum and you want stronger guarantees for part of your API, Typeway integrates directly. No migration. No separate service. Nest a type-safe API group alongside your existing routes and get compile-time verification where it matters most.

The API is the type. The rest follows.

---

*Typeway is open source at [github.com/joshburgess/typeway](https://github.com/joshburgess/typeway). It requires stable Rust 1.80+.*
