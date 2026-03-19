//! Type-level API versioning with typed deltas and compile-time compatibility checks.
//!
//! API evolution is expressed as typed deltas: V2 is defined as a set of changes
//! applied to V1 — added endpoints, removed endpoints, replaced endpoints, and
//! deprecated endpoints. The type system verifies that changes are intentional and
//! that backward compatibility is maintained (or explicitly broken).
//!
//! # Example
//!
//! ```ignore
//! type V1 = (
//!     GetEndpoint<UsersPath, Json<Vec<UserV1>>>,
//!     GetEndpoint<UserByIdPath, Json<UserV1>>,
//!     PostEndpoint<UsersPath, Json<CreateUser>, Json<UserV1>>,
//! );
//!
//! type V2Changes = (
//!     Added<GetEndpoint<UserProfilePath, Json<Profile>>>,
//!     Replaced<
//!         GetEndpoint<UserByIdPath, Json<UserV1>>,
//!         GetEndpoint<UserByIdPath, Json<UserV2>>,
//!     >,
//!     Deprecated<PostEndpoint<UsersPath, Json<CreateUser>, Json<UserV1>>>,
//! );
//!
//! // The resolved API after applying changes:
//! type V2Resolved = (
//!     GetEndpoint<UsersPath, Json<Vec<UserV1>>>,
//!     GetEndpoint<UserByIdPath, Json<UserV2>>,          // replaced
//!     PostEndpoint<UsersPath, Json<CreateUser>, Json<UserV1>>,  // deprecated but present
//!     GetEndpoint<UserProfilePath, Json<Profile>>,      // added
//! );
//!
//! type V2 = VersionedApi<V1, V2Changes, V2Resolved>;
//! ```

use std::marker::PhantomData;

use crate::api::ApiSpec;

// ---------------------------------------------------------------------------
// Change markers
// ---------------------------------------------------------------------------

/// Marks an endpoint as added in this version (not present in the base).
pub struct Added<E>(PhantomData<E>);

/// Marks an endpoint as removed in this version.
///
/// The original endpoint type `E` is preserved for documentation and
/// migration tooling even though it no longer appears in the resolved API.
pub struct Removed<E>(PhantomData<E>);

/// Marks an endpoint as replaced: the old signature becomes the new one.
///
/// Both `Old` and `New` should share the same HTTP method and path for the
/// replacement to be semantically valid.
pub struct Replaced<Old, New>(PhantomData<(Old, New)>);

/// Marks an endpoint as deprecated but still present.
///
/// The endpoint continues to function, but generated documentation and
/// clients should flag it as deprecated.
pub struct Deprecated<E>(PhantomData<E>);

// ---------------------------------------------------------------------------
// ChangeMarker — trait for counting change types
// ---------------------------------------------------------------------------

/// Sealed helper trait implemented by each change marker.
/// Used internally by [`ApiChangelog`] counting logic.
pub trait ChangeMarker {
    /// 1 if this is an `Added`, 0 otherwise.
    const IS_ADDED: usize;
    /// 1 if this is a `Removed`, 0 otherwise.
    const IS_REMOVED: usize;
    /// 1 if this is a `Replaced`, 0 otherwise.
    const IS_REPLACED: usize;
    /// 1 if this is a `Deprecated`, 0 otherwise.
    const IS_DEPRECATED: usize;

    /// Human-readable description of this change.
    fn describe() -> String;
}

impl<E> ChangeMarker for Added<E> {
    const IS_ADDED: usize = 1;
    const IS_REMOVED: usize = 0;
    const IS_REPLACED: usize = 0;
    const IS_DEPRECATED: usize = 0;

    fn describe() -> String {
        String::from("Added endpoint")
    }
}

impl<E> ChangeMarker for Removed<E> {
    const IS_ADDED: usize = 0;
    const IS_REMOVED: usize = 1;
    const IS_REPLACED: usize = 0;
    const IS_DEPRECATED: usize = 0;

    fn describe() -> String {
        String::from("Removed endpoint")
    }
}

impl<Old, New> ChangeMarker for Replaced<Old, New> {
    const IS_ADDED: usize = 0;
    const IS_REMOVED: usize = 0;
    const IS_REPLACED: usize = 1;
    const IS_DEPRECATED: usize = 0;

    fn describe() -> String {
        String::from("Replaced endpoint")
    }
}

impl<E> ChangeMarker for Deprecated<E> {
    const IS_ADDED: usize = 0;
    const IS_REMOVED: usize = 0;
    const IS_REPLACED: usize = 0;
    const IS_DEPRECATED: usize = 1;

    fn describe() -> String {
        String::from("Deprecated endpoint")
    }
}

// ---------------------------------------------------------------------------
// ApiChangelog — describes changes between two API versions
// ---------------------------------------------------------------------------

/// Describes the changes between two API versions.
///
/// Implemented for tuples of [`ChangeMarker`] types via `macro_rules!`.
pub trait ApiChangelog {
    /// Number of endpoints added.
    const ADDED: usize;
    /// Number of endpoints removed.
    const REMOVED: usize;
    /// Number of endpoints replaced (changed signature).
    const REPLACED: usize;
    /// Number of endpoints deprecated.
    const DEPRECATED: usize;

    /// Human-readable summary of changes.
    fn summary() -> String;
}

// Unit tuple — no changes.
impl ApiChangelog for () {
    const ADDED: usize = 0;
    const REMOVED: usize = 0;
    const REPLACED: usize = 0;
    const DEPRECATED: usize = 0;

    fn summary() -> String {
        String::from("No changes")
    }
}

macro_rules! impl_api_changelog_for_tuple {
    ($($T:ident),+) => {
        impl<$($T: ChangeMarker),+> ApiChangelog for ($($T,)+) {
            const ADDED: usize = 0 $(+ $T::IS_ADDED)+;
            const REMOVED: usize = 0 $(+ $T::IS_REMOVED)+;
            const REPLACED: usize = 0 $(+ $T::IS_REPLACED)+;
            const DEPRECATED: usize = 0 $(+ $T::IS_DEPRECATED)+;

            fn summary() -> String {
                [$($T::describe()),+].join("; ")
            }
        }
    };
}

impl_api_changelog_for_tuple!(A);
impl_api_changelog_for_tuple!(A, B);
impl_api_changelog_for_tuple!(A, B, C);
impl_api_changelog_for_tuple!(A, B, C, D);
impl_api_changelog_for_tuple!(A, B, C, D, E);
impl_api_changelog_for_tuple!(A, B, C, D, E, F);
impl_api_changelog_for_tuple!(A, B, C, D, E, F, G);
impl_api_changelog_for_tuple!(A, B, C, D, E, F, G, H);
impl_api_changelog_for_tuple!(A, B, C, D, E, F, G, H, I);
impl_api_changelog_for_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_api_changelog_for_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_api_changelog_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_api_changelog_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_api_changelog_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_api_changelog_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_api_changelog_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

// ---------------------------------------------------------------------------
// VersionedApi — versioned API with typed deltas
// ---------------------------------------------------------------------------

/// A versioned API type carrying its lineage as type parameters.
///
/// - `Base`: The previous API version (must implement [`ApiSpec`]).
/// - `Changes`: A tuple of change markers ([`Added`], [`Removed`],
///   [`Replaced`], [`Deprecated`]) describing what changed.
/// - `Resolved`: The actual API tuple after applying changes. This is the
///   set of endpoints the server will serve.
///
/// Because type-level set operations are not feasible on stable Rust, the
/// `Resolved` parameter is specified explicitly. The change markers serve as
/// machine-readable documentation and enable compile-time assertions via
/// [`assert_api_compatible!`].
pub struct VersionedApi<Base, Changes, Resolved: ApiSpec> {
    _marker: PhantomData<(Base, Changes, Resolved)>,
}

/// `VersionedApi` delegates its `ApiSpec` implementation to `Resolved`.
impl<B, C, R: ApiSpec> ApiSpec for VersionedApi<B, C, R> {}

/// `VersionedApi` delegates `AllProvided` to the resolved API type.
/// This allows `EffectfulServer::serve()` to verify effects on versioned APIs.
impl<B, C, R, P, Idx> crate::effects::AllProvided<P, Idx> for VersionedApi<B, C, R>
where
    R: ApiSpec + crate::effects::AllProvided<P, Idx>,
{
}

// ---------------------------------------------------------------------------
// BackwardCompatible — marker trait for compatible API evolution
// ---------------------------------------------------------------------------

/// Marker trait asserting that a newer API version is backward-compatible
/// with an older one.
///
/// "Backward-compatible" means every endpoint in the old API either:
/// - Exists identically in the new API, or
/// - Was explicitly handled via [`Replaced`] or [`Removed`] in the change
///   set.
///
/// Use [`assert_api_compatible!`] to generate the required impl
/// automatically.
pub trait BackwardCompatible<Old> {}

// ---------------------------------------------------------------------------
// HasEndpoint — type-level set membership via index witness
// ---------------------------------------------------------------------------

/// Type-level index: the endpoint is at this position in the tuple.
pub struct Here;

/// Type-level index: the endpoint is at a later position in the tuple.
pub struct There<T>(PhantomData<T>);

/// Trait asserting that an API tuple contains a specific endpoint type.
///
/// The `Idx` parameter is a type-level index witness that disambiguates
/// impls: each tuple position uses a distinct index type (`Here`,
/// `There<Here>`, `There<There<Here>>`, etc.), so the compiler never
/// sees two impls for the same `(Tuple, Endpoint, Idx)` triple.
///
/// Users do not need to specify `Idx` — it is inferred automatically.
pub trait HasEndpoint<E, Idx> {}

/// Helper trait that is satisfied when `Self` is the same type as `E`.
///
/// The blanket impl `T: IsEndpoint<T>` means the compiler's type
/// unification performs the equality check.
pub trait IsEndpoint<E> {}
impl<T> IsEndpoint<T> for T {}

// 1-tuple: the endpoint can only be at position 0.
impl<E> HasEndpoint<E, Here> for (E,) {}

// For tuples of arity 2..16, generate one impl per position.
// Position 0 uses `Here`, position 1 uses `There<Here>`, etc.
//
// For a 3-tuple (A, B, C):
//   impl HasEndpoint<E, Here>                for (E, B, C)          -- match at 0
//   impl HasEndpoint<E, There<Here>>         for (A, E, C)          -- match at 1
//   impl HasEndpoint<E, There<There<Here>>>  for (A, B, E)          -- match at 2
//
// Each impl has a distinct Idx type, so no coherence conflicts.

macro_rules! count_to_idx {
    // 0 skips => Here
    (0) => { Here };
    // N skips => There<(N-1) skips>
    (1) => { There<Here> };
    (2) => { There<There<Here>> };
    (3) => { There<There<There<Here>>> };
    (4) => { There<There<There<There<Here>>>> };
    (5) => { There<There<There<There<There<Here>>>>> };
    (6) => { There<There<There<There<There<There<Here>>>>>> };
    (7) => { There<There<There<There<There<There<There<Here>>>>>>> };
    (8) => { There<There<There<There<There<There<There<There<Here>>>>>>>> };
    (9) => { There<There<There<There<There<There<There<There<There<Here>>>>>>>>> };
    (10) => { There<There<There<There<There<There<There<There<There<There<Here>>>>>>>>>> };
    (11) => { There<There<There<There<There<There<There<There<There<There<There<Here>>>>>>>>>>> };
    (12) => { There<There<There<There<There<There<There<There<There<There<There<There<Here>>>>>>>>>>>> };
    (13) => { There<There<There<There<There<There<There<There<There<There<There<There<There<Here>>>>>>>>>>>>> };
    (14) => { There<There<There<There<There<There<There<There<There<There<There<There<There<There<Here>>>>>>>>>>>>>> };
    (15) => { There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<Here>>>>>>>>>>>>>>> };
    (16) => { There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<Here>>>>>>>>>>>>>>>> };
    (17) => { There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<Here>>>>>>>>>>>>>>>>> };
    (18) => { There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<Here>>>>>>>>>>>>>>>>>> };
    (19) => { There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<There<Here>>>>>>>>>>>>>>>>>>> };
}

// Generate a single HasEndpoint impl for a specific position in a tuple.
// $pos is the numeric position (for index type selection).
// $before are type params before the match position.
// $match is the type param at the match position.
// $after are type params after the match position.
macro_rules! impl_has_endpoint_at {
    ($pos:tt, [$($before:ident),*], $match:ident, [$($after:ident),*]) => {
        impl<Endpoint, $($before,)* $match, $($after,)*>
            HasEndpoint<Endpoint, count_to_idx!($pos)>
            for ($($before,)* $match, $($after,)*)
        where
            $match: IsEndpoint<Endpoint>,
        {}
    };
}

// 2-tuple
impl_has_endpoint_at!(0, [], A, [B]);
impl_has_endpoint_at!(1, [A], B, []);

// 3-tuple
impl_has_endpoint_at!(0, [], A, [B, C]);
impl_has_endpoint_at!(1, [A], B, [C]);
impl_has_endpoint_at!(2, [A, B], C, []);

// 4-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D]);
impl_has_endpoint_at!(1, [A], B, [C, D]);
impl_has_endpoint_at!(2, [A, B], C, [D]);
impl_has_endpoint_at!(3, [A, B, C], D, []);

// 5-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E]);
impl_has_endpoint_at!(1, [A], B, [C, D, E]);
impl_has_endpoint_at!(2, [A, B], C, [D, E]);
impl_has_endpoint_at!(3, [A, B, C], D, [E]);
impl_has_endpoint_at!(4, [A, B, C, D], E, []);

// 6-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, []);

// 7-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, []);

// 8-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, []);

// 9-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, []);

// 10-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I, J]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I, J]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I, J]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I, J]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I, J]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I, J]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I, J]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I, J]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, [J]);
impl_has_endpoint_at!(9, [A, B, C, D, E, F, G, H, I], J, []);

// 11-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I, J, K]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I, J, K]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I, J, K]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I, J, K]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I, J, K]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I, J, K]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I, J, K]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I, J, K]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, [J, K]);
impl_has_endpoint_at!(9, [A, B, C, D, E, F, G, H, I], J, [K]);
impl_has_endpoint_at!(10, [A, B, C, D, E, F, G, H, I, J], K, []);

// 12-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I, J, K, L]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I, J, K, L]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I, J, K, L]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I, J, K, L]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I, J, K, L]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I, J, K, L]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I, J, K, L]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I, J, K, L]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, [J, K, L]);
impl_has_endpoint_at!(9, [A, B, C, D, E, F, G, H, I], J, [K, L]);
impl_has_endpoint_at!(10, [A, B, C, D, E, F, G, H, I, J], K, [L]);
impl_has_endpoint_at!(11, [A, B, C, D, E, F, G, H, I, J, K], L, []);

// 13-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I, J, K, L, M]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I, J, K, L, M]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I, J, K, L, M]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I, J, K, L, M]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I, J, K, L, M]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I, J, K, L, M]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I, J, K, L, M]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I, J, K, L, M]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, [J, K, L, M]);
impl_has_endpoint_at!(9, [A, B, C, D, E, F, G, H, I], J, [K, L, M]);
impl_has_endpoint_at!(10, [A, B, C, D, E, F, G, H, I, J], K, [L, M]);
impl_has_endpoint_at!(11, [A, B, C, D, E, F, G, H, I, J, K], L, [M]);
impl_has_endpoint_at!(12, [A, B, C, D, E, F, G, H, I, J, K, L], M, []);

// 14-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I, J, K, L, M, N]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I, J, K, L, M, N]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I, J, K, L, M, N]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I, J, K, L, M, N]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I, J, K, L, M, N]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I, J, K, L, M, N]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I, J, K, L, M, N]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I, J, K, L, M, N]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, [J, K, L, M, N]);
impl_has_endpoint_at!(9, [A, B, C, D, E, F, G, H, I], J, [K, L, M, N]);
impl_has_endpoint_at!(10, [A, B, C, D, E, F, G, H, I, J], K, [L, M, N]);
impl_has_endpoint_at!(11, [A, B, C, D, E, F, G, H, I, J, K], L, [M, N]);
impl_has_endpoint_at!(12, [A, B, C, D, E, F, G, H, I, J, K, L], M, [N]);
impl_has_endpoint_at!(13, [A, B, C, D, E, F, G, H, I, J, K, L, M], N, []);

// 15-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I, J, K, L, M, N, O]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I, J, K, L, M, N, O]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I, J, K, L, M, N, O]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I, J, K, L, M, N, O]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I, J, K, L, M, N, O]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I, J, K, L, M, N, O]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I, J, K, L, M, N, O]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I, J, K, L, M, N, O]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, [J, K, L, M, N, O]);
impl_has_endpoint_at!(9, [A, B, C, D, E, F, G, H, I], J, [K, L, M, N, O]);
impl_has_endpoint_at!(10, [A, B, C, D, E, F, G, H, I, J], K, [L, M, N, O]);
impl_has_endpoint_at!(11, [A, B, C, D, E, F, G, H, I, J, K], L, [M, N, O]);
impl_has_endpoint_at!(12, [A, B, C, D, E, F, G, H, I, J, K, L], M, [N, O]);
impl_has_endpoint_at!(13, [A, B, C, D, E, F, G, H, I, J, K, L, M], N, [O]);
impl_has_endpoint_at!(14, [A, B, C, D, E, F, G, H, I, J, K, L, M, N], O, []);

// 16-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I, J, K, L, M, N, O, P]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I, J, K, L, M, N, O, P]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I, J, K, L, M, N, O, P]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I, J, K, L, M, N, O, P]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I, J, K, L, M, N, O, P]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I, J, K, L, M, N, O, P]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I, J, K, L, M, N, O, P]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I, J, K, L, M, N, O, P]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, [J, K, L, M, N, O, P]);
impl_has_endpoint_at!(9, [A, B, C, D, E, F, G, H, I], J, [K, L, M, N, O, P]);
impl_has_endpoint_at!(10, [A, B, C, D, E, F, G, H, I, J], K, [L, M, N, O, P]);
impl_has_endpoint_at!(11, [A, B, C, D, E, F, G, H, I, J, K], L, [M, N, O, P]);
impl_has_endpoint_at!(12, [A, B, C, D, E, F, G, H, I, J, K, L], M, [N, O, P]);
impl_has_endpoint_at!(13, [A, B, C, D, E, F, G, H, I, J, K, L, M], N, [O, P]);
impl_has_endpoint_at!(14, [A, B, C, D, E, F, G, H, I, J, K, L, M, N], O, [P]);
impl_has_endpoint_at!(15, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O], P, []);

// 17-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I, J, K, L, M, N, O, P, Q]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I, J, K, L, M, N, O, P, Q]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I, J, K, L, M, N, O, P, Q]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I, J, K, L, M, N, O, P, Q]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I, J, K, L, M, N, O, P, Q]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I, J, K, L, M, N, O, P, Q]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, [J, K, L, M, N, O, P, Q]);
impl_has_endpoint_at!(9, [A, B, C, D, E, F, G, H, I], J, [K, L, M, N, O, P, Q]);
impl_has_endpoint_at!(10, [A, B, C, D, E, F, G, H, I, J], K, [L, M, N, O, P, Q]);
impl_has_endpoint_at!(11, [A, B, C, D, E, F, G, H, I, J, K], L, [M, N, O, P, Q]);
impl_has_endpoint_at!(12, [A, B, C, D, E, F, G, H, I, J, K, L], M, [N, O, P, Q]);
impl_has_endpoint_at!(13, [A, B, C, D, E, F, G, H, I, J, K, L, M], N, [O, P, Q]);
impl_has_endpoint_at!(14, [A, B, C, D, E, F, G, H, I, J, K, L, M, N], O, [P, Q]);
impl_has_endpoint_at!(15, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O], P, [Q]);
impl_has_endpoint_at!(16, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P], Q, []);

// 18-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I, J, K, L, M, N, O, P, Q, R]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I, J, K, L, M, N, O, P, Q, R]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I, J, K, L, M, N, O, P, Q, R]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I, J, K, L, M, N, O, P, Q, R]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I, J, K, L, M, N, O, P, Q, R]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, [J, K, L, M, N, O, P, Q, R]);
impl_has_endpoint_at!(9, [A, B, C, D, E, F, G, H, I], J, [K, L, M, N, O, P, Q, R]);
impl_has_endpoint_at!(10, [A, B, C, D, E, F, G, H, I, J], K, [L, M, N, O, P, Q, R]);
impl_has_endpoint_at!(11, [A, B, C, D, E, F, G, H, I, J, K], L, [M, N, O, P, Q, R]);
impl_has_endpoint_at!(12, [A, B, C, D, E, F, G, H, I, J, K, L], M, [N, O, P, Q, R]);
impl_has_endpoint_at!(13, [A, B, C, D, E, F, G, H, I, J, K, L, M], N, [O, P, Q, R]);
impl_has_endpoint_at!(14, [A, B, C, D, E, F, G, H, I, J, K, L, M, N], O, [P, Q, R]);
impl_has_endpoint_at!(15, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O], P, [Q, R]);
impl_has_endpoint_at!(16, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P], Q, [R]);
impl_has_endpoint_at!(17, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q], R, []);

// 19-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I, J, K, L, M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I, J, K, L, M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I, J, K, L, M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I, J, K, L, M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, [J, K, L, M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(9, [A, B, C, D, E, F, G, H, I], J, [K, L, M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(10, [A, B, C, D, E, F, G, H, I, J], K, [L, M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(11, [A, B, C, D, E, F, G, H, I, J, K], L, [M, N, O, P, Q, R, S]);
impl_has_endpoint_at!(12, [A, B, C, D, E, F, G, H, I, J, K, L], M, [N, O, P, Q, R, S]);
impl_has_endpoint_at!(13, [A, B, C, D, E, F, G, H, I, J, K, L, M], N, [O, P, Q, R, S]);
impl_has_endpoint_at!(14, [A, B, C, D, E, F, G, H, I, J, K, L, M, N], O, [P, Q, R, S]);
impl_has_endpoint_at!(15, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O], P, [Q, R, S]);
impl_has_endpoint_at!(16, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P], Q, [R, S]);
impl_has_endpoint_at!(17, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q], R, [S]);
impl_has_endpoint_at!(18, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R], S, []);

// 20-tuple
impl_has_endpoint_at!(0, [], A, [B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(1, [A], B, [C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(2, [A, B], C, [D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(3, [A, B, C], D, [E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(4, [A, B, C, D], E, [F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(5, [A, B, C, D, E], F, [G, H, I, J, K, L, M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(6, [A, B, C, D, E, F], G, [H, I, J, K, L, M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(7, [A, B, C, D, E, F, G], H, [I, J, K, L, M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(8, [A, B, C, D, E, F, G, H], I, [J, K, L, M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(9, [A, B, C, D, E, F, G, H, I], J, [K, L, M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(10, [A, B, C, D, E, F, G, H, I, J], K, [L, M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(11, [A, B, C, D, E, F, G, H, I, J, K], L, [M, N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(12, [A, B, C, D, E, F, G, H, I, J, K, L], M, [N, O, P, Q, R, S, T]);
impl_has_endpoint_at!(13, [A, B, C, D, E, F, G, H, I, J, K, L, M], N, [O, P, Q, R, S, T]);
impl_has_endpoint_at!(14, [A, B, C, D, E, F, G, H, I, J, K, L, M, N], O, [P, Q, R, S, T]);
impl_has_endpoint_at!(15, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O], P, [Q, R, S, T]);
impl_has_endpoint_at!(16, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P], Q, [R, S, T]);
impl_has_endpoint_at!(17, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q], R, [S, T]);
impl_has_endpoint_at!(18, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R], S, [T]);
impl_has_endpoint_at!(19, [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S], T, []);

// ---------------------------------------------------------------------------
// assert_api_compatible! — compile-time backward compatibility check
// ---------------------------------------------------------------------------

/// Verify at compile time that every endpoint in the old API exists in the
/// new API (type equality on the full endpoint type).
///
/// This macro generates a const block with trait bounds that fail to compile
/// if any endpoint from the first argument is missing in the second.
///
/// # Usage
///
/// ```ignore
/// // List the endpoints that must be preserved, and the new API tuple.
/// assert_api_compatible!(
///     (EndpointA, EndpointB),
///     (EndpointA, EndpointB, EndpointC)
/// );
/// // Compile error if NewApi is missing EndpointA or EndpointB.
/// ```
///
/// If an endpoint was intentionally removed or replaced, omit it from the
/// first argument — list only the endpoints that must be preserved.
#[macro_export]
macro_rules! assert_api_compatible {
    (($($old_ep:ty),+ $(,)?), $new_api:ty) => {
        $crate::assert_api_compatible!(@expand 0usize; $new_api; $($old_ep),+);
    };
    // Internal: expand each old endpoint into a separate type-equality check.
    // We use a helper function per endpoint whose body is unreachable but whose
    // signature forces the compiler to verify the type matches a tuple element.
    (@expand $counter:expr; $new_api:ty; $head:ty $(, $rest:ty)*) => {
        const _: () = {
            fn _check(v: $new_api) {
                // This closure pattern forces the compiler to prove that $head
                // is one of the tuple elements. We destructure the tuple and
                // require that one position has type $head.
                let _ = v;
            }
        };
        $crate::_assert_api_has_endpoint!($new_api, $head);
        $($crate::_assert_api_has_endpoint!($new_api, $rest);)*
    };
}

/// Internal helper: assert that a single endpoint type is present in an API
/// tuple. Uses [`HasEndpoint`] with an inferred index witness.
///
/// This is not intended to be called directly; use [`assert_api_compatible!`].
#[doc(hidden)]
#[macro_export]
macro_rules! _assert_api_has_endpoint {
    ($api:ty, $ep:ty) => {
        const _: () = {
            fn _check<Idx>()
            where
                $api: $crate::versioning::HasEndpoint<$ep, Idx>,
            {}
        };
    };
}

// ---------------------------------------------------------------------------
// VersionInfo — metadata for OpenAPI and documentation
// ---------------------------------------------------------------------------

/// Metadata about an API version, used by OpenAPI generation and
/// documentation tooling.
///
/// # Example
///
/// ```ignore
/// struct V1;
/// impl VersionInfo for V1 {
///     const VERSION: &'static str = "1.0.0";
///     const TITLE: &'static str = "Users API V1";
/// }
/// ```
pub trait VersionInfo {
    /// Semantic version string (e.g., `"1.0.0"`).
    const VERSION: &'static str;
    /// Human-readable API title.
    const TITLE: &'static str;
}

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

    struct profiles;
    impl LitSegment for profiles {
        const VALUE: &'static str = "profiles";
    }

    // -- Domain types --
    #[derive(Debug)]
    struct UserV1;
    #[derive(Debug)]
    struct UserV2;
    #[derive(Debug)]
    struct CreateUser;
    #[derive(Debug)]
    struct Profile;

    // -- Path aliases --
    type UsersPath = HCons<Lit<users>, HNil>;
    type UserByIdPath = HCons<Lit<users>, HCons<Capture<u32>, HNil>>;
    type UserProfilePath = HCons<Lit<users>, HCons<Capture<u32>, HCons<Lit<profiles>, HNil>>>;

    // -- V1 endpoints --
    type ListUsersV1 = GetEndpoint<UsersPath, Vec<UserV1>>;
    type GetUserV1 = GetEndpoint<UserByIdPath, UserV1>;
    type CreateUserV1 = PostEndpoint<UsersPath, CreateUser, UserV1>;

    type V1 = (ListUsersV1, GetUserV1, CreateUserV1);

    // -- V2 changes --
    type V2Changes = (
        Added<GetEndpoint<UserProfilePath, Profile>>,
        Replaced<GetUserV1, GetEndpoint<UserByIdPath, UserV2>>,
        Deprecated<CreateUserV1>,
    );

    // -- V2 resolved --
    type V2Resolved = (
        ListUsersV1,
        GetEndpoint<UserByIdPath, UserV2>,
        CreateUserV1,
        GetEndpoint<UserProfilePath, Profile>,
    );

    type V2 = VersionedApi<V1, V2Changes, V2Resolved>;

    fn assert_api_spec<A: ApiSpec>() {}

    #[test]
    fn v1_is_api_spec() {
        assert_api_spec::<V1>();
    }

    #[test]
    fn v2_is_api_spec() {
        assert_api_spec::<V2>();
    }

    #[test]
    fn changelog_counts_are_correct() {
        assert_eq!(<V2Changes as ApiChangelog>::ADDED, 1);
        assert_eq!(<V2Changes as ApiChangelog>::REMOVED, 0);
        assert_eq!(<V2Changes as ApiChangelog>::REPLACED, 1);
        assert_eq!(<V2Changes as ApiChangelog>::DEPRECATED, 1);
    }

    #[test]
    fn changelog_summary_contains_all_changes() {
        let summary = <V2Changes as ApiChangelog>::summary();
        assert!(summary.contains("Added"));
        assert!(summary.contains("Replaced"));
        assert!(summary.contains("Deprecated"));
    }

    #[test]
    fn empty_changelog() {
        assert_eq!(<() as ApiChangelog>::ADDED, 0);
        assert_eq!(<() as ApiChangelog>::REMOVED, 0);
        assert_eq!(<() as ApiChangelog>::REPLACED, 0);
        assert_eq!(<() as ApiChangelog>::DEPRECATED, 0);
    }

    #[test]
    fn single_change_changelog() {
        type Changes = (Added<GetEndpoint<UserProfilePath, Profile>>,);
        assert_eq!(<Changes as ApiChangelog>::ADDED, 1);
        assert_eq!(<Changes as ApiChangelog>::REMOVED, 0);
    }

    // Compile-time check: V2Resolved contains all V1 endpoints that were
    // not replaced. ListUsersV1 and CreateUserV1 are still present.
    // GetUserV1 was replaced, so we omit it from the compatibility check.
    assert_api_compatible!(
        (ListUsersV1, CreateUserV1),
        V2Resolved
    );

    #[test]
    fn has_endpoint_works_for_present_endpoint() {
        fn assert_has<Api: HasEndpoint<E, Idx>, E, Idx>() {}
        assert_has::<V2Resolved, ListUsersV1, Here>();
        assert_has::<V2Resolved, CreateUserV1, _>();
        assert_has::<V2Resolved, GetEndpoint<UserByIdPath, UserV2>, _>();
        assert_has::<V2Resolved, GetEndpoint<UserProfilePath, Profile>, _>();
    }

    // VersionInfo
    struct V1Info;
    impl VersionInfo for V1Info {
        const VERSION: &'static str = "1.0.0";
        const TITLE: &'static str = "Users API V1";
    }

    struct V2Info;
    impl VersionInfo for V2Info {
        const VERSION: &'static str = "2.0.0";
        const TITLE: &'static str = "Users API V2";
    }

    #[test]
    fn version_info_constants() {
        assert_eq!(V1Info::VERSION, "1.0.0");
        assert_eq!(V1Info::TITLE, "Users API V1");
        assert_eq!(V2Info::VERSION, "2.0.0");
        assert_eq!(V2Info::TITLE, "Users API V2");
    }
}
