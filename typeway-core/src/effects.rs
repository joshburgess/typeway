//! Type-level middleware effects for compile-time enforcement.
//!
//! Middleware requirements are encoded as type-level effects. An endpoint
//! wrapped in [`Requires<E, Endpoint>`] declares that it needs a specific
//! middleware. The server builder's `.provide::<E>()` method discharges the
//! requirement, and `serve()` only compiles when all effects are discharged.
//!
//! # Example
//!
//! ```ignore
//! use typeway_core::effects::*;
//!
//! // Declare an API with middleware requirements:
//! type API = (
//!     Requires<AuthRequired, GetEndpoint<UserPath, User>>,
//!     Requires<CorsRequired, GetEndpoint<PublicPath, Data>>,
//!     GetEndpoint<HealthPath, String>,  // no requirements
//! );
//!
//! // On the server side:
//! EffectfulServer::<API>::new(handlers)
//!     .provide::<AuthRequired>()   // discharge auth requirement
//!     .layer(auth_layer)           // apply the actual middleware
//!     .provide::<CorsRequired>()   // discharge CORS requirement
//!     .layer(CorsLayer::permissive())
//!     .serve(addr)                 // compiles only if all effects discharged
//!     .await;
//! ```

use std::marker::PhantomData;

use crate::api::ApiSpec;

// ---------------------------------------------------------------------------
// Effect trait
// ---------------------------------------------------------------------------

/// Marker trait for middleware effects.
///
/// Implement this for zero-sized types that represent middleware requirements.
///
/// # Example
///
/// ```ignore
/// struct MyCustomEffect;
/// impl Effect for MyCustomEffect {}
/// ```
pub trait Effect: Send + Sync + 'static {}

// ---------------------------------------------------------------------------
// Requires<E, Endpoint> — declare a middleware requirement
// ---------------------------------------------------------------------------

/// An endpoint that requires a specific middleware effect.
///
/// The server must `.provide::<E>()` before serving, or compilation fails.
///
/// `E` is the effect marker type (e.g., [`AuthRequired`]).
/// `T` is the underlying endpoint or API type.
///
/// # Example
///
/// ```ignore
/// type SecureEndpoint = Requires<AuthRequired, GetEndpoint<UserPath, User>>;
/// ```
pub struct Requires<E: Effect, T> {
    _marker: PhantomData<(E, T)>,
}

impl<E: Effect, T: ApiSpec> ApiSpec for Requires<E, T> {}

// ---------------------------------------------------------------------------
// Built-in effect markers
// ---------------------------------------------------------------------------

/// Authentication middleware required.
pub struct AuthRequired;
impl Effect for AuthRequired {}

/// Rate limiting middleware required.
pub struct RateLimitRequired;
impl Effect for RateLimitRequired {}

/// CORS middleware required.
pub struct CorsRequired;
impl Effect for CorsRequired {}

/// Tracing/logging middleware required.
pub struct TracingRequired;
impl Effect for TracingRequired {}

// ---------------------------------------------------------------------------
// Type-level effect list
// ---------------------------------------------------------------------------

/// Type-level empty effect list.
///
/// Represents the state where no effects have been provided yet.
pub struct ENil;

/// Type-level cons cell for effect lists.
///
/// Each `.provide::<E>()` call adds `E` to the front of the provided
/// effects list: `ECons<E, PreviouslyProvided>`.
pub struct ECons<E: Effect, Tail>(PhantomData<(E, Tail)>);

// ---------------------------------------------------------------------------
// HasEffect — type-level set membership via index witness
// ---------------------------------------------------------------------------

/// Type-level index: the effect is at this position in the list.
pub struct EHere;

/// Type-level index: the effect is at a later position in the list.
pub struct EThere<T>(PhantomData<T>);

/// Trait asserting that an effect list contains a specific effect.
///
/// Uses the same index witness technique as [`HasEndpoint`](crate::versioning::HasEndpoint)
/// to avoid coherence conflicts. The `Idx` parameter disambiguates impls
/// and is inferred automatically.
#[diagnostic::on_unimplemented(
    message = "effect `{E}` has not been provided",
    label = "this effect list does not contain `{E}`",
    note = "call `.provide::<{E}>()` on the EffectfulServer to discharge this requirement"
)]
pub trait HasEffect<E: Effect, Idx> {}

/// The effect at the head of the list matches.
impl<E: Effect, Tail> HasEffect<E, EHere> for ECons<E, Tail> {}

/// The effect is somewhere in the tail of the list.
impl<E: Effect, Head: Effect, Tail, Idx> HasEffect<E, EThere<Idx>> for ECons<Head, Tail>
where
    Tail: HasEffect<E, Idx>,
{
}

// ---------------------------------------------------------------------------
// AllProvided — assert all effects in an API type are discharged
// ---------------------------------------------------------------------------

/// Asserts that all effects required by an API type are present in the
/// provided effects list `P`.
///
/// This trait is the key compile-time check: `serve()` requires
/// `A: AllProvided<P, Idx>`, which recursively verifies that every
/// [`Requires<E, _>`] wrapper in the API type has a corresponding `E`
/// in the provided effects list.
///
/// The `Idx` parameter is a composite index witness that carries the
/// proof structure — it is inferred automatically and never specified
/// by users.
///
/// # How it works
///
/// - Plain endpoints (`Endpoint<...>`) have no requirements, so
///   `AllProvided<P, ()>` holds for any `P`.
/// - `Requires<E, T>` requires `P: HasEffect<E, I>` AND `T: AllProvided<P, J>`,
///   with `Idx = (I, J)`.
/// - Tuples require every element to satisfy `AllProvided<P, _>`.
#[diagnostic::on_unimplemented(
    message = "not all middleware effects have been provided for API type `{Self}`",
    label = "some required effects are missing",
    note = "ensure every `Requires<E, _>` in the API has a corresponding `.provide::<E>()` call"
)]
pub trait AllProvided<P, Idx> {}

// Plain endpoints have no requirements.
impl<M, P, Req, Res, Q, Err, Provided> AllProvided<Provided, ()>
    for crate::endpoint::Endpoint<M, P, Req, Res, Q, Err>
where
    M: crate::method::HttpMethod,
    P: crate::path::PathSpec,
{
}

// Requires<E, T> needs E to be in the provided list, and T must also satisfy AllProvided.
// The index witness is a pair (I, J) where I proves HasEffect and J proves the inner AllProvided.
impl<E, T, P, I, J> AllProvided<P, (I, J)> for Requires<E, T>
where
    E: Effect,
    T: AllProvided<P, J>,
    P: HasEffect<E, I>,
{
}

// Unit tuple — no endpoints, no requirements.
impl<P> AllProvided<P, ()> for () {}

// Tuples of AllProvided types are AllProvided.
// Each element has its own index witness, collected into a tuple.
macro_rules! impl_all_provided_for_tuple {
    ($($T:ident, $I:ident);+) => {
        impl<Provided, $($T, $I,)+> AllProvided<Provided, ($($I,)+)> for ($($T,)+)
        where $($T: AllProvided<Provided, $I>,)+ {}
    };
}

impl_all_provided_for_tuple!(A, IA);
impl_all_provided_for_tuple!(A, IA; B, IB);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN; O, IO);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN; O, IO; P, IP);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN; O, IO; P, IP; Q, IQ);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN; O, IO; P, IP; Q, IQ; R, IR);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN; O, IO; P, IP; Q, IQ; R, IR; S, IS);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN; O, IO; P, IP; Q, IQ; R, IR; S, IS; T, IT);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN; O, IO; P, IP; Q, IQ; R, IR; S, IS; T, IT; U, IU);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN; O, IO; P, IP; Q, IQ; R, IR; S, IS; T, IT; U, IU; V, IV);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN; O, IO; P, IP; Q, IQ; R, IR; S, IS; T, IT; U, IU; V, IV; W, IW);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN; O, IO; P, IP; Q, IQ; R, IR; S, IS; T, IT; U, IU; V, IV; W, IW; X, IX);
impl_all_provided_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ; K, IK; L, IL; M, IM; N, IN; O, IO; P, IP; Q, IQ; R, IR; S, IS; T, IT; U, IU; V, IV; W, IW; X, IX; Y, IY);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(non_camel_case_types, dead_code)]
mod tests {
    use super::*;
    use crate::endpoint::*;
    use crate::path::*;

    // -- Literal segment markers --
    struct users;
    impl LitSegment for users {
        const VALUE: &'static str = "users";
    }

    struct health;
    impl LitSegment for health {
        const VALUE: &'static str = "health";
    }

    // -- Domain types --
    #[derive(Debug)]
    struct User;

    // -- Path aliases --
    type UsersPath = HCons<Lit<users>, HNil>;
    type HealthPath = HCons<Lit<health>, HNil>;
    type UserByIdPath = HCons<Lit<users>, HCons<Capture<u32>, HNil>>;

    // -- Custom effect --
    struct CustomEffect;
    impl Effect for CustomEffect {}

    // -- Helper functions --
    fn assert_api_spec<A: ApiSpec>() {}
    fn assert_effect<E: Effect>() {}
    fn assert_has_effect<L: HasEffect<E, Idx>, E: Effect, Idx>() {}
    fn assert_all_provided<A: AllProvided<P, Idx>, P, Idx>() {}

    // -- Tests --

    #[test]
    fn effect_markers_implement_effect() {
        assert_effect::<AuthRequired>();
        assert_effect::<RateLimitRequired>();
        assert_effect::<CorsRequired>();
        assert_effect::<TracingRequired>();
        assert_effect::<CustomEffect>();
    }

    #[test]
    fn requires_wrapping_preserves_api_spec() {
        type E = GetEndpoint<UsersPath, Vec<User>>;
        assert_api_spec::<E>();
        assert_api_spec::<Requires<AuthRequired, E>>();
        assert_api_spec::<Requires<CorsRequired, Requires<AuthRequired, E>>>();
    }

    #[test]
    fn requires_in_api_tuple_is_api_spec() {
        type API = (
            Requires<AuthRequired, GetEndpoint<UserByIdPath, User>>,
            GetEndpoint<HealthPath, String>,
        );
        assert_api_spec::<API>();
    }

    #[test]
    fn has_effect_finds_effect_at_head() {
        type List = ECons<AuthRequired, ENil>;
        assert_has_effect::<List, AuthRequired, EHere>();
    }

    #[test]
    fn has_effect_finds_effect_in_tail() {
        type List = ECons<CorsRequired, ECons<AuthRequired, ENil>>;
        assert_has_effect::<List, AuthRequired, _>();
    }

    #[test]
    fn has_effect_finds_effect_deep() {
        type List = ECons<TracingRequired, ECons<CorsRequired, ECons<AuthRequired, ENil>>>;
        assert_has_effect::<List, AuthRequired, _>();
        assert_has_effect::<List, CorsRequired, _>();
        assert_has_effect::<List, TracingRequired, _>();
    }

    #[test]
    fn plain_endpoint_all_provided_for_any_list() {
        type E = GetEndpoint<UsersPath, Vec<User>>;
        assert_all_provided::<E, ENil, _>();
        assert_all_provided::<E, ECons<AuthRequired, ENil>, _>();
    }

    #[test]
    fn requires_endpoint_all_provided_when_effect_present() {
        type E = Requires<AuthRequired, GetEndpoint<UsersPath, Vec<User>>>;
        assert_all_provided::<E, ECons<AuthRequired, ENil>, _>();
    }

    #[test]
    fn requires_multiple_effects_all_provided() {
        type E = Requires<CorsRequired, Requires<AuthRequired, GetEndpoint<UsersPath, Vec<User>>>>;
        type Provided = ECons<CorsRequired, ECons<AuthRequired, ENil>>;
        assert_all_provided::<E, Provided, _>();
    }

    #[test]
    fn api_tuple_all_provided() {
        type API = (
            Requires<AuthRequired, GetEndpoint<UserByIdPath, User>>,
            Requires<CorsRequired, GetEndpoint<UsersPath, Vec<User>>>,
            GetEndpoint<HealthPath, String>,
        );
        type Provided = ECons<CorsRequired, ECons<AuthRequired, ENil>>;
        assert_all_provided::<API, Provided, _>();
    }

    #[test]
    fn order_of_provided_does_not_matter() {
        type API = (
            Requires<AuthRequired, GetEndpoint<UserByIdPath, User>>,
            Requires<CorsRequired, GetEndpoint<UsersPath, Vec<User>>>,
        );
        // Auth first, then CORS
        type P1 = ECons<AuthRequired, ECons<CorsRequired, ENil>>;
        assert_all_provided::<API, P1, _>();
        // CORS first, then Auth
        type P2 = ECons<CorsRequired, ECons<AuthRequired, ENil>>;
        assert_all_provided::<API, P2, _>();
    }

    #[test]
    fn extra_provided_effects_are_fine() {
        type API = (Requires<AuthRequired, GetEndpoint<UserByIdPath, User>>,);
        type Provided = ECons<TracingRequired, ECons<CorsRequired, ECons<AuthRequired, ENil>>>;
        assert_all_provided::<API, Provided, _>();
    }

    #[test]
    fn empty_api_all_provided_for_any_list() {
        assert_all_provided::<(), ENil, _>();
        assert_all_provided::<(), ECons<AuthRequired, ENil>, _>();
    }

    #[test]
    fn single_endpoint_tuple_all_provided() {
        type API = (GetEndpoint<HealthPath, String>,);
        assert_all_provided::<API, ENil, _>();
    }

    #[test]
    fn duplicate_effects_in_api_only_need_one_provide() {
        // Two endpoints require AuthRequired — one .provide::<AuthRequired>() suffices.
        type API = (
            Requires<AuthRequired, GetEndpoint<UserByIdPath, User>>,
            Requires<AuthRequired, GetEndpoint<UsersPath, Vec<User>>>,
        );
        type Provided = ECons<AuthRequired, ENil>;
        assert_all_provided::<API, Provided, _>();
    }
}
