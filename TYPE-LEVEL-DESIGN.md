# Type-Level Design: Features, Trade-offs, and Future Direction

This document describes the Rust type system features that Typeway relies on, the workarounds it uses for features that aren't yet stable, and how future language improvements could simplify the framework.

## Features Typeway Uses Today (Stable Rust)

### Trait-Level Computation

Typeway's core mechanism: traits with associated types that compute results at compile time. The `PathSpec` trait recurses over an HList to produce a capture tuple type:

```rust
impl PathSpec for HNil { type Captures = (); }
impl<U, T: PathSpec> PathSpec for HCons<Capture<U>, T>
where T::Captures: Prepend<U> {
    type Captures = <T::Captures as Prepend<U>>::Output;
}
```

This is a type-level catamorphism (fold). The compiler evaluates it fully at compile time — no runtime cost.

**Status:** Stable since Rust 1.0. This is the foundation and won't change.

### PhantomData Marker Types

Zero-sized types that exist only for the type checker. `Endpoint<M, P, Req, Res, Q, Err>` carries six type parameters but has zero runtime representation:

```rust
pub struct Endpoint<M: HttpMethod, P: PathSpec, Req, Res, Q = (), Err = ()> {
    _marker: PhantomData<(M, P, Req, Res, Q, Err)>,
}
```

**Status:** Stable since Rust 1.0.

### Default Type Parameters

Endpoint's `Q` and `Err` parameters default to `()`, preserving backward compatibility:

```rust
// These are the same type:
GetEndpoint<UsersPath, Json<Vec<User>>>
GetEndpoint<UsersPath, Json<Vec<User>>, (), ()>
```

**Status:** Stable since Rust 1.0.

### Macro-Generated Flat Impls

Instead of recursive trait resolution (which causes exponential compile times in Haskell's Servant), Typeway generates flat impls for tuple arities 1–20 via `macro_rules!`:

```rust
macro_rules! impl_serves_for_tuple {
    ($(($E:ident, $idx:tt)),+) => {
        impl<$($E: ApiSpec,)+> Serves<($($E,)+)> for ($(BoundHandler<$E>,)+) {
            fn register(self, router: &mut Router) {
                $(self.$idx.register_into(router);)+
            }
        }
    };
}
```

This is O(1) per instantiation for the compiler — no recursive constraint solving.

**Status:** Stable since Rust 1.0.

### `#[diagnostic::on_unimplemented]`

Custom error messages when trait bounds aren't satisfied:

```rust
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as an HTTP response",
    label = "does not implement `IntoResponse`",
    note = "valid response types: `&'static str`, `String`, `Json<T>`, ..."
)]
pub trait IntoResponse { ... }
```

**Status:** Stable since Rust 1.78.

### Proc Macros

`typeway_path!`, `typeway_api!`, `endpoint!`, `#[handler]`, and `#[api_description]` are proc macros that generate types and validation code. The most important is `typeway_path!`, which works around the absence of const generic strings by generating marker types:

```rust
typeway_path!(type UsersPath = "users" / u32);
// Generates:
// mod __wp_UsersPath {
//     pub struct __lit_users;
//     impl LitSegment for __lit_users { const VALUE: &'static str = "users"; }
// }
// type UsersPath = HCons<Lit<__wp_UsersPath::__lit_users>, HCons<Capture<u32>, HNil>>;
```

**Status:** Stable since Rust 1.30.

## What Typeway Works Around (Unstable Features)

### Const Generic `&'static str` — The Big One

**Feature:** `adt_const_params` — allows `&'static str` as a const generic parameter.

**What we do instead:** Each unique path literal generates a zero-sized marker type implementing `LitSegment`. The proc macro `typeway_path!` automates this. Without it, users would have to define marker types by hand.

**What it would enable:**

```rust
// Current (marker types via proc macro):
typeway_path!(type UsersPath = "users" / u32);

// With const generic strings (no macro needed):
type UsersPath = HCons<Lit<"users">, HCons<Capture<u32>, HNil>>;
```

**Concrete improvements:**
- Eliminate `typeway_path!` macro entirely — paths become plain type expressions
- No hidden `__wp_*` modules polluting `cargo doc` output
- Two paths with the same literal are automatically the same type (currently they're different types in different modules, requiring the same macro invocation)
- Simpler compiler error messages: `Lit<"users">` instead of `Lit<__wp_UsersPath::__lit_users>`
- The `LitSegment` trait, all marker type generation, and the module-scoping machinery become unnecessary

**Why we can't use it:** The `adt_const_params` feature has been unstable since 2021 with no clear stabilization timeline. Depending on it would require nightly Rust, which excludes most production users.

**Adoption plan:** When stabilized, add a `const-generics` feature flag that enables `Lit<"string">` syntax alongside the existing macro approach. The macro approach remains the default for backward compatibility.

### Specialization

**Feature:** `specialization` / `min_specialization` — allows overlapping trait impls where a more specific impl takes priority.

**What we do instead:** Separate traits for different handler patterns (`Handler`, `AuthHandler`, `StrictHandler`) with marker types (`WithBody<Parts, Body>`, `AuthWithBody<Parts, Body>`) to disambiguate overlapping arities.

**What it would enable:** A single `Handler` trait with specialization for auth-required and strict-return-type variants. The `WithBody` marker disambiguation would be unnecessary.

**Why we can't use it:** `specialization` has been unstable since 2016 and has known soundness issues. `min_specialization` is more conservative but still unstable.

**Impact if stabilized:** Moderate. Would reduce the number of handler traits from 3 to 1 and eliminate the marker types. The user-facing API wouldn't change much.

### Negative Trait Impls

**Feature:** The ability to say "this type does NOT implement trait X."

**What we do instead:** The `NotUnset` marker trait in the experimental type-level builder (Option C), which had to be manually implemented for every user type.

**What it would enable:** Blanket impl `impl<T: !Unset> NotUnset for T {}` — any type that isn't `Unset` automatically satisfies the bound. This would make the type-level builder pattern (Option C) viable.

**Why we can't use it:** Negative impls have been discussed since 2015 but never stabilized. Coherence implications are complex.

**Impact if stabilized:** Would make the type-level builder ergonomic enough to recommend over the `endpoint!` macro. Currently, Option C is impractical precisely because of this limitation.

## Features We Could Use But Choose Not To

### GATs (Generic Associated Types) — Stable Since 1.65

GATs allow associated types to have their own generic parameters:

```rust
trait StreamingExtractor {
    type Body<'a>: AsyncRead + 'a;
    fn extract<'a>(req: &'a mut Request) -> Self::Body<'a>;
}
```

**Why we don't use them:** The core design (HList paths, endpoint tuples, trait-based dispatch) doesn't benefit from GATs. The patterns we use — flat tuple impls via macros, PhantomData markers — are simpler and produce better error messages. GATs would add complexity without improving the user-facing API.

**Where they could help:** Custom streaming extractors that borrow from the request. Currently, `FromRequest` returns owned data (`Bytes`). A GAT-based version could return borrowed data with a lifetime tied to the request. This is a potential future enhancement for advanced use cases, not a core framework change.

**Trade-off:** GATs increase trait complexity and tend to produce harder-to-read error messages. The compile-time cost of resolving GAT bounds is also higher than flat impls. For a framework where compile time and error quality are explicit priorities, this trade-off isn't worth it for the core API.

### Const Generics for Integers — Stable Since 1.51

Integer const generics allow types parameterized by numbers:

```rust
struct RateLimited<const MAX: u32, const WINDOW_SECS: u64, E>(PhantomData<E>);
type Limited = RateLimited<100, 60, GetEndpoint<UsersPath, String>>;
```

**Why we don't use them (much):** The trait-based approach is more readable in practice:

```rust
// Const generics: concise but opaque
type E = RateLimited<100, 60, GetEndpoint<...>>;

// Trait-based: more lines to define, but self-documenting at use site
struct StandardRate;
impl RateLimit for StandardRate {
    const MAX_REQUESTS: u32 = 100;
    const WINDOW_SECS: u64 = 60;
}
type E = RateLimited<StandardRate, GetEndpoint<...>>;
```

The trait approach names the configuration (`StandardRate`), making it reusable and readable. The const generic approach is terser but anonymous — you can't tell what `100, 60` means without context.

**Where they could help:** For simple, well-understood constants (max body size, timeout duration) where a named type would be overkill. We may adopt this selectively for specific wrappers.

## Summary

| Feature | Status | Impact | Adopted? |
|---------|--------|--------|----------|
| Trait-level computation | Stable | Foundation of the framework | Yes |
| PhantomData markers | Stable | Zero-cost type parameters | Yes |
| Default type parameters | Stable | Backward-compatible endpoint evolution | Yes |
| Macro-generated flat impls | Stable | O(1) compile time per instantiation | Yes |
| `#[diagnostic::on_unimplemented]` | Stable | Clear compile errors | Yes |
| Proc macros | Stable | `typeway_path!`, `endpoint!`, `#[handler]` | Yes |
| Const generic `&'static str` | **Unstable** | Eliminates marker types and path macros | Waiting for stabilization |
| Specialization | **Unstable** | Unifies handler traits | Waiting; moderate impact |
| Negative trait impls | **Unstable** | Enables type-level builder (Option C) | Waiting; high impact |
| GATs | Stable | Streaming extractors | No — complexity exceeds benefit for core API |
| Const generic integers | Stable | Terser rate limit / config types | Selectively, where naming isn't needed |

The framework is designed so that unstable features, when they stabilize, can be adopted incrementally behind feature flags without breaking existing code. The macro-based approach (`typeway_path!`, `endpoint!`) will remain supported even after const generic strings stabilize — it's syntactic sugar that some users may prefer regardless.
