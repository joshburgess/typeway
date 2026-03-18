# Spike Review: Type Theory Analysis & Research Ideation

Review of the Phase 0 spike (Steps 0.2–0.4) covering type-level path encoding,
method types, and handler matching. This document captures findings that should
inform DESIGN.md and all subsequent implementation phases.

---

## Part 1: Type-Theoretic Analysis

### What the Spike Is Actually Encoding

The spike simulates **dependent types in Rust's trait system**. Here's the mapping:

| Spike construct | Type-theoretic analog |
|---|---|
| `HCons<H, T>` / `HNil` | Inductive type-level list (μ-type at kind level) |
| `Lit<S>` / `Segment<T>` | Sum-type elements of the path algebra |
| `ExtractSegments` | Type-level fold (catamorphism) over the path HList |
| `Prepend<T>` | Type-level cons operation on tuples |
| `Route<M, P, Req, Res>` | Product kind — a 4-tuple at the type level |
| `Handler<R>` | Type-indexed function space (approximated Π-type) |

`Handler<Route<M, P, Req, Res>>` is approximately a Π-type — "for this specific
route type, the handler must have *this* shape." Rust can't express Π-types
directly, so the spike uses trait-level computation (`ExtractSegments`) to project
from the route type to the handler's expected signature.

### Verdict: HList vs Tuples for Paths

**HList is correct for paths. Keep it.**

Paths are inherently recursive: match one segment, recurse on the remainder.
This is a catamorphism — exactly what HList is built for. `ExtractSegments`
implements this catamorphism cleanly: base case at `HNil`, structural recursion
at `HCons`.

Flat tuples would require matching on the entire structure at once, creating a
combinatorial explosion of impls (`(Lit, Segment, Lit)`, `(Segment, Lit, Segment, Lit)`,
etc.). HList gives `O(n)` impls via recursion.

**However: use flat tuples for the API type** (the collection of routes). Routes
don't have recursive structure; they're a flat set. The plan already does this
(`type API = (Route1, Route2, Route3)`).

**Rule: HList for paths, tuples for APIs.**

### Verdict: Prepend<T> — Sound

The `Prepend<T>` trait is a type-level cons operation on tuples, used to
accumulate captures as the HList catamorphism unfolds left-to-right:

```
HCons<Segment<A>, HCons<Lit<"x">, HCons<Segment<B>, HNil>>>
  → Prepend<A> applied to (Prepend<B> applied to ())
  → Prepend<A> applied to (B,)
  → (A, B)
```

First capture appears first in the tuple. Correct.

**Subtlety**: The `where T::Captures: Prepend<U>` bound creates a chain of
dependent constraints. For a path with `n` captures, the compiler solves `n`
nested `Prepend` obligations. This is `O(n)` per route — fine for paths (n ≤ 8).
Verify it doesn't interact badly with the `O(m)` API-level tuple expansion
(m = number of routes). Total should be `O(n × m)`, not `O(n^m)`.

### Verdict: Marker Types for Literals — Pragmatically Right

The `LitSegment` trait + marker type approach is the correct stable-Rust choice.

**Why the alternatives are worse:**

- **Const generic `[u8; N]`**: Can't distinguish different byte arrays of the same
  length in trait resolution without const-generic expression equality (not stable).

- **Typenum-style character encoding**: Type-level natural numbers for each character
  composed into an HList. Compiles but constraint solving overhead is catastrophic —
  `O(k)` trait resolutions per `k`-character string.

- **Const generic `&'static str`**: Requires nightly `adt_const_params`. Not acceptable.

**Marker types**: One ZST per unique literal string, proc-macro-generated. Constraint
solver sees each as a distinct opaque type — `O(1)` resolution. Proc-macro hides
the verbosity.

**Refinement**: Namespace generated marker types in a private module to avoid collisions:

```rust
mod __wayward_lit {
    pub struct users;
    pub struct posts;
}
type Path = HCons<Lit<__wayward_lit::users>, HNil>;
```

### Problem: The Handler Adapter Pattern (H1/H2) Must Be Replaced

The `H1<F>` / `H2<F>` newtype adapter pattern is the **weakest part of the spike**.

**Root cause**: Rust's trait coherence rules prevent overlapping blanket impls.
You can't write:

```rust
impl<F: Fn() -> R> Handler<Route<..., Captures=()>> for F { ... }
impl<F: Fn(A) -> R> Handler<Route<..., Captures=(A,)>> for F { ... }
```

The compiler can't prove these don't overlap (it doesn't reason about Fn trait
exclusivity).

Newtypes disambiguate but create an architectural problem: **who decides which
wrapper to use?** The proc-macro would need to analyze the route type's capture
tuple arity at macro expansion time, before type checking. Fragile.

**Solution: the Axum "Captures as Extractor" pattern.** See Idea 4 in Part 2.

### Problem: Res Type Placement in Route

`Route<M, P, Req, Res>` conflates API description with handler contract. In
practice, handlers return `Result<Res, Error>` or `impl IntoResponse`, not
the exact `Res` type.

**Recommendation: two-level split.**

```rust
// API-level: what the HTTP interface looks like (for OpenAPI/clients)
struct Endpoint<M, P, Req, Res> { ... }

// Handler-level: what the function actually returns
trait Handler<E: EndpointSpec> {
    type Output: IntoResponse + CompatibleWith<E::Res>;
}
```

`CompatibleWith` checks that the handler's output can produce the declared
response type. Handlers can return error types, use `impl IntoResponse`, etc.,
while the happy-path type matches the API spec for OpenAPI/client generation.

### Composition Properties

**Composes well:**
- Path composition: `HCons` is associative up to type equality.
- API composition: Tuple concatenation via `Append` trait merges sub-APIs.
- Middleware stacking: Tower's `Layer` trait composes naturally.

**Does not compose well (needs work):**
- Nested/sub-routing: `WithPrefix<"api/v1", UsersAPI>` needs a type-level map
  over tuples to prepend path segments to all routes. Requires recursive trait
  or proc-macro expansion.

### Soundness

No soundness issues. All type-level computation uses trait associated types,
which Rust's type checker guarantees are deterministic. No `unsafe`, no variance
tricks, no transmute-adjacent reasoning. PhantomData usage is correct (covariant
in all parameters, appropriate for pure description types).

---

## Part 2: Research Ideation — Novel Directions

### Idea 1: Type-Level Middleware as an Effect System

**Pitch**: Encode middleware requirements as type-level *effects* that handlers
declare and the server builder *discharges*, catching missing middleware at
compile time.

Tower middleware is currently untyped — you can forget auth middleware and the
compiler won't notice. But middleware is structurally identical to an algebraic
effect handler: it intercepts a computation, provides some capability, and wraps
the result.

```rust
trait Effect {}
struct Authed;
struct RateLimited;
struct Traced;

type API = (
    Endpoint<Get, path!("health"), (), String>,
    Requires<Authed, Endpoint<Get, path!("users"), (), Vec<User>>>,
    Requires<(Authed, RateLimited), Endpoint<Post, path!("users"), CreateUser, User>>,
);

Server::<API>::new(handlers)
    .handle::<Authed>(auth_middleware)
    .handle::<RateLimited>(rate_limiter)
    // compile error if any effects remain undischarged
    .serve(addr)
```

`serve()` requires `AllEffectsDischarged<API>`, a trait that walks the API type
and verifies every `Requires<E, ...>` has a corresponding `.handle::<E>(...)`.

**What's hard**: Encoding the "set of discharged effects" as a type that grows
with each `.handle()` call. Implementable via sorted type-level lists with a
`Contains` trait.

**Novelty**: Existing effect system research focuses on computation effects.
Applying the discipline to infrastructure concerns (middleware) is unexplored
in the PL literature. Nothing like this exists in the Rust ecosystem.

### Idea 2: Session-Typed WebSocket Routes

**Pitch**: Encode WebSocket protocols as session types in the route spec, so
the compiler verifies handlers follow the protocol.

```rust
type ChatProtocol = Recv<JoinMsg, Send<WelcomeMsg,
    Offer<
        Recv<ChatMsg, Send<BroadcastMsg, Recurse>>,
        Recv<LeaveMsg, End>
    >>>;

type API = (
    get!("chat", WebSocket<ChatProtocol>),
);

async fn chat_handler(ws: TypedWebSocket<ChatProtocol>) {
    let join = ws.recv().await;          // returns JoinMsg
    let ws = ws.send(WelcomeMsg).await;  // must send WelcomeMsg next
    // compiler prevents wrong message type or wrong order
}
```

Each state transition *consumes* the old channel and produces a new one at the
next session type. Rust's ownership system enforces the linear discipline
naturally.

**What's hard**: `Recurse` requires equi-recursive types or a fixpoint at the
type level. Rust doesn't have equi-recursive types, but a trait-level indirection
(`trait Protocol { type Unfolded; }`) can approximate it.

**Impact**: Would be a genuine first for Rust web frameworks. Closest work is in
research languages (Scribble, Links).

### Idea 3: Content Negotiation as Type-Level Coproduct

**Pitch**: Route response types are coproducts of all possible representations;
the framework negotiates automatically based on the `Accept` header.

```rust
type API = (
    get!("users" / u32, Negotiated<(Json<User>, Xml<User>, Html<UserView>, Csv<UserRow>)>),
);
```

OpenAPI derives all content types. The handler returns a `User`, and the framework
inspects `Accept`, selects the best representation, and serializes.

`Negotiated<(A, B, C)>` is a type-level coproduct. Each variant must be derivable
from the same domain type, enforced by `From<DomainType>` bounds.

**Difficulty**: Easy — straightforward coproduct encoding.

### Idea 4: Captures as Extractor (Replaces H1/H2) ★ RECOMMENDED

**Pitch**: Instead of matching handler arity against route captures, make
`Path<P>` an extractor type and use Axum-style arity-based Handler impls.

```rust
struct Path<P: PathSpec>(P::Captures);

// Handler impls on extractor types, not capture tuples
impl<F, Fut, Res> Handler for F
where F: FnOnce() -> Fut, Fut: Future<Output = Res>, Res: IntoResponse { ... }

impl<F, Fut, T1, Res> Handler for F
where F: FnOnce(T1) -> Fut, T1: FromRequestParts, Res: IntoResponse { ... }

// User writes:
async fn get_user(Path(id): Path<path!("users" / u32)>) -> Json<User> { ... }

// Or with proc-macro sugar:
async fn get_user(id: u32) -> Json<User> { ... }
```

Arity impls don't overlap because they're on different extractor types (`Path`,
`Json`, `State`), not raw function types. Axum proves this compiles on stable Rust.

**Bridge to the Route type** via a `Compatible` trait:

```rust
trait Compatible<Route> {}
impl<P: PathSpec> Compatible<Route<_, P, _, _>> for Path<P> {}
```

Server builder checks `Compatible` for each handler-route pair, catching
mismatches at compile time without needing H1/H2.

**This eliminates the weakest part of the spike design.**

### Idea 5: Type-Level API Versioning

**Pitch**: Express API evolution as type-level operations with compile-time
compatibility checking.

```rust
type V1 = (
    get!("users" / u32, Json<UserV1>),
);

type V2 = Extends<V1, (
    get!("users" / u32 / "profile", Json<Profile>),
    Replaces<get!("users" / u32, Json<UserV1>), get!("users" / u32, Json<UserV2>)>,
)>;
```

`Extends<Base, Changes>` verifies all `Replaces` items exist in the base,
produces the merged API type, and can generate compatibility-checking clients.

**What's hard**: Type-level set difference operation. Implementable but needs
careful trait engineering to avoid exponential constraint solving.

### Feasibility Summary

| Idea | Novelty | Difficulty | Impact | Priority |
|---|---|---|---|---|
| Middleware effects | High | Medium | High | Phase 7+ |
| Session-typed WS | High | Hard | Medium (niche) | Post-launch research |
| Content negotiation | Medium | Easy | Medium | Phase 5 or 6 |
| **Captures as extractor** | **Low (proven)** | **Easy** | **Very high** | **Phase 1 — do now** |
| API versioning | High | Hard | Medium | Future |

---

## Architectural Decisions for DESIGN.md

Based on this review, the following decisions should be adopted:

1. **HList for paths, flat tuples for APIs.** Do not change this.

2. **Marker types + proc-macro for literal segments.** Namespace in `__wayward_lit`.

3. **Adopt the Captures-as-Extractor pattern.** Eliminate H1/H2 adapters. Use
   Axum-style extractor-based handler dispatch with a `Compatible<Route>` bridge
   for compile-time verification against the API spec.

4. **Split Endpoint (API spec) from Handler contract.** Handlers return
   `impl IntoResponse`; a `CompatibleWith<Res>` trait verifies the happy-path
   type matches the declared response for OpenAPI/client generation.

5. **Middleware effects as flagship novel feature.** Design in Phase 2, implement
   in Phase 7+. This is the differentiator.

6. **Session-typed WebSockets as stretch goal.** Research project, not blocking.

7. **Content negotiation coproducts fit naturally in the OpenAPI phase.** Add to
   Phase 5 scope.
