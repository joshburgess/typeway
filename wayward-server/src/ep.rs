//! Type-level endpoint builder (experimental).
//!
//! Provides [`Ep`] — a builder struct with defaulted type parameters that
//! constructs endpoint types through associated type "setters". Each setter
//! returns a new `Ep` with one more parameter filled in. The [`Build`] trait
//! converts the final `Ep` into the real nested wrapper types.
//!
//! # Why this exists
//!
//! Rust doesn't have type-level method chaining. The `endpoint!` macro
//! (Option B) is the ergonomic way to build endpoints. This module
//! (Option C) is an experiment in how far Rust's trait system can go.
//!
//! # Usage
//!
//! ```ignore
//! use wayward_server::ep::*;
//!
//! // Simple GET
//! type GetUsers = <Ep<Get, UsersPath, Res = Json<Vec<User>>> as Build>::Out;
//!
//! // With builder traits (associated type chaining):
//! type CreateUser = <
//!     <
//!         <Ep<Post, UsersPath> as WithRes<Json<User>>>::Out
//!             as WithReq<Json<CreateUser>>
//!     >::Out as WithAuth<AuthUser>
//! >::Out;
//!
//! // ...which is why endpoint!() macro exists.
//! ```
//!
//! # Honest assessment
//!
//! The nested `< <X as Trait>::Out as Trait>::Out` syntax is worse than
//! manual wrapper nesting. This module exists to prove the concept and
//! to demonstrate Rust's type system limitations. Use `endpoint!()` instead.

use std::marker::PhantomData;

use wayward_core::{ApiSpec, HttpMethod, PathSpec};

// ---------------------------------------------------------------------------
// Sentinel types for unset parameters
// ---------------------------------------------------------------------------

/// Marker for an unset builder parameter.
pub struct Unset;

// ---------------------------------------------------------------------------
// The builder struct
// ---------------------------------------------------------------------------

/// Type-level endpoint builder.
///
/// All optional parameters default to [`Unset`]. Use the `With*` traits
/// to fill them in, then [`Build`] to produce the final endpoint type.
pub struct Ep<
    M: HttpMethod,
    P: PathSpec,
    Req = Unset,
    Res = Unset,
    Q = Unset,
    Err = Unset,
    Auth = Unset,
    Val = Unset,
    Ct = Unset,
    Ver = Unset,
    Rl = Unset,
    Str = Unset,
> {
    _marker: PhantomData<(M, P, Req, Res, Q, Err, Auth, Val, Ct, Ver, Rl, Str)>,
}

// ---------------------------------------------------------------------------
// Setter traits — each returns a new Ep with one param changed
// ---------------------------------------------------------------------------

/// Set the request body type.
pub trait WithReq<T> {
    type Out;
}

impl<M: HttpMethod, P: PathSpec, Res, Q, Err, Auth, Val, Ct, Ver, Rl, Str, T> WithReq<T>
    for Ep<M, P, Unset, Res, Q, Err, Auth, Val, Ct, Ver, Rl, Str>
{
    type Out = Ep<M, P, T, Res, Q, Err, Auth, Val, Ct, Ver, Rl, Str>;
}

/// Set the response type.
pub trait WithRes<T> {
    type Out;
}

impl<M: HttpMethod, P: PathSpec, Req, Q, Err, Auth, Val, Ct, Ver, Rl, Str, T> WithRes<T>
    for Ep<M, P, Req, Unset, Q, Err, Auth, Val, Ct, Ver, Rl, Str>
{
    type Out = Ep<M, P, Req, T, Q, Err, Auth, Val, Ct, Ver, Rl, Str>;
}

/// Set the query parameter type.
pub trait WithQuery<T> {
    type Out;
}

impl<M: HttpMethod, P: PathSpec, Req, Res, Err, Auth, Val, Ct, Ver, Rl, Str, T> WithQuery<T>
    for Ep<M, P, Req, Res, Unset, Err, Auth, Val, Ct, Ver, Rl, Str>
{
    type Out = Ep<M, P, Req, Res, T, Err, Auth, Val, Ct, Ver, Rl, Str>;
}

/// Set the error response type.
pub trait WithErr<T> {
    type Out;
}

impl<M: HttpMethod, P: PathSpec, Req, Res, Q, Auth, Val, Ct, Ver, Rl, Str, T> WithErr<T>
    for Ep<M, P, Req, Res, Q, Unset, Auth, Val, Ct, Ver, Rl, Str>
{
    type Out = Ep<M, P, Req, Res, Q, T, Auth, Val, Ct, Ver, Rl, Str>;
}

/// Set the auth extractor type (wraps in `Protected`).
pub trait WithAuth<T> {
    type Out;
}

impl<M: HttpMethod, P: PathSpec, Req, Res, Q, Err, Val, Ct, Ver, Rl, Str, T> WithAuth<T>
    for Ep<M, P, Req, Res, Q, Err, Unset, Val, Ct, Ver, Rl, Str>
{
    type Out = Ep<M, P, Req, Res, Q, Err, T, Val, Ct, Ver, Rl, Str>;
}

/// Set the body validator type (wraps in `Validated`).
pub trait WithValidation<T> {
    type Out;
}

impl<M: HttpMethod, P: PathSpec, Req, Res, Q, Err, Auth, Ct, Ver, Rl, Str, T> WithValidation<T>
    for Ep<M, P, Req, Res, Q, Err, Auth, Unset, Ct, Ver, Rl, Str>
{
    type Out = Ep<M, P, Req, Res, Q, Err, Auth, T, Ct, Ver, Rl, Str>;
}

/// Set the content-type constraint (wraps in `ContentType`).
pub trait WithContentType<T> {
    type Out;
}

impl<M: HttpMethod, P: PathSpec, Req, Res, Q, Err, Auth, Val, Ver, Rl, Str, T> WithContentType<T>
    for Ep<M, P, Req, Res, Q, Err, Auth, Val, Unset, Ver, Rl, Str>
{
    type Out = Ep<M, P, Req, Res, Q, Err, Auth, Val, T, Ver, Rl, Str>;
}

/// Set the API version (wraps in `Versioned`).
pub trait WithVersion<T> {
    type Out;
}

impl<M: HttpMethod, P: PathSpec, Req, Res, Q, Err, Auth, Val, Ct, Rl, Str, T> WithVersion<T>
    for Ep<M, P, Req, Res, Q, Err, Auth, Val, Ct, Unset, Rl, Str>
{
    type Out = Ep<M, P, Req, Res, Q, Err, Auth, Val, Ct, T, Rl, Str>;
}

/// Set the rate limit (wraps in `RateLimited`).
pub trait WithRateLimit<T> {
    type Out;
}

impl<M: HttpMethod, P: PathSpec, Req, Res, Q, Err, Auth, Val, Ct, Ver, Str, T> WithRateLimit<T>
    for Ep<M, P, Req, Res, Q, Err, Auth, Val, Ct, Ver, Unset, Str>
{
    type Out = Ep<M, P, Req, Res, Q, Err, Auth, Val, Ct, Ver, T, Str>;
}

/// Marker type that enables `Strict` wrapping.
pub struct Enabled;

/// Enable strict return type checking (wraps in `Strict`).
pub trait MakeStrict {
    type Out;
}

impl<M: HttpMethod, P: PathSpec, Req, Res, Q, Err, Auth, Val, Ct, Ver, Rl> MakeStrict
    for Ep<M, P, Req, Res, Q, Err, Auth, Val, Ct, Ver, Rl, Unset>
{
    type Out = Ep<M, P, Req, Res, Q, Err, Auth, Val, Ct, Ver, Rl, Enabled>;
}

// ---------------------------------------------------------------------------
// Build — convert Ep<...> into the actual endpoint type
// ---------------------------------------------------------------------------

/// Convert a fully-specified `Ep` builder into the real endpoint type
/// with all wrappers applied.
pub trait Build {
    type Out;
}

/// Helper: resolves Unset to a default, or uses the provided type.
trait ResolveDefault<Default> {
    type Resolved;
}

impl<D> ResolveDefault<D> for Unset {
    type Resolved = D;
}

// Blanket: any non-Unset type resolves to itself.
// Can't do this due to orphan rules / overlap with Unset.
// Instead, we'll implement Build for specific Ep configurations.

// The fundamental problem: we can't pattern-match on "is this Unset or not"
// in a blanket impl. So we implement Build for the most common configurations.

// --- Minimal: Ep<M, P, Unset, Res, ...> (no body, no wrappers) ---
impl<M: HttpMethod, P: PathSpec, Res> Build for Ep<M, P, Unset, Res> {
    type Out = wayward_core::Endpoint<M, P, wayward_core::NoBody, Res>;
}

// --- With body: Ep<M, P, Req, Res> ---
impl<M: HttpMethod, P: PathSpec, Req, Res> Build
    for Ep<M, P, Req, Res, Unset, Unset, Unset, Unset, Unset, Unset, Unset, Unset>
where
    // Exclude Req = Unset (handled above with 4-param version)
    Req: NotUnset,
{
    type Out = wayward_core::Endpoint<M, P, Req, Res>;
}

// --- With errors ---
impl<M: HttpMethod, P: PathSpec, Req, Res, Err> Build
    for Ep<M, P, Req, Res, Unset, Err, Unset, Unset, Unset, Unset, Unset, Unset>
where
    Req: ReqOrNoBody,
    Err: NotUnset,
{
    type Out = wayward_core::Endpoint<M, P, Req::Resolved, Res, (), Err>;
}

// --- With auth ---
impl<M: HttpMethod, P: PathSpec, Req, Res, Err, Auth> Build
    for Ep<M, P, Req, Res, Unset, Err, Auth, Unset, Unset, Unset, Unset, Unset>
where
    Req: ReqOrNoBody,
    Err: ErrOrUnit,
    Auth: NotUnset,
{
    type Out = crate::auth::Protected<
        Auth,
        wayward_core::Endpoint<M, P, Req::Resolved, Res, (), Err::Resolved>,
    >;
}

/// Helper: maps Unset → NoBody, anything else → itself.
pub trait ReqOrNoBody {
    type Resolved;
}

impl ReqOrNoBody for Unset {
    type Resolved = wayward_core::NoBody;
}

// Blanket for all non-Unset types — use a marker trait
pub trait NotUnset {}

// Implement NotUnset for common types. Can't do a blanket `impl<T> NotUnset for T`
// because it would conflict with the specialized impls for Unset.
// THIS IS THE FUNDAMENTAL LIMITATION OF OPTION C.
//
// Users must either:
// 1. Use the endpoint!() macro instead (Option B — recommended)
// 2. Implement NotUnset for their custom types
//
// We provide impls for all common types and a simple marker trait.
impl NotUnset for () {}
impl<T> NotUnset for crate::response::Json<T> {}
impl<T> NotUnset for Vec<T> {}
impl<T> NotUnset for Option<T> {}
impl NotUnset for String {}
impl NotUnset for bytes::Bytes {}
impl NotUnset for crate::error::JsonError {}
impl NotUnset for http::StatusCode {}
impl NotUnset for u8 {}
impl NotUnset for u16 {}
impl NotUnset for u32 {}
impl NotUnset for u64 {}
impl NotUnset for i32 {}
impl NotUnset for i64 {}
impl NotUnset for f32 {}
impl NotUnset for f64 {}
impl NotUnset for bool {}
impl NotUnset for &'static str {}

impl<T: NotUnset> ReqOrNoBody for T {
    type Resolved = T;
}

/// Helper: maps Unset → (), anything else → itself.
pub trait ErrOrUnit {
    type Resolved;
}

impl ErrOrUnit for Unset {
    type Resolved = ();
}

impl<T: NotUnset> ErrOrUnit for T {
    type Resolved = T;
}

// ---------------------------------------------------------------------------
// Convenience type aliases
// ---------------------------------------------------------------------------

/// Start building a GET endpoint.
pub type GET<P, Res> = Ep<wayward_core::Get, P, Unset, Res>;

/// Start building a POST endpoint.
pub type POST<P> = Ep<wayward_core::Post, P>;

/// Start building a PUT endpoint.
pub type PUT<P> = Ep<wayward_core::Put, P>;

/// Start building a DELETE endpoint.
pub type DELETE<P, Res> = Ep<wayward_core::Delete, P, Unset, Res>;
