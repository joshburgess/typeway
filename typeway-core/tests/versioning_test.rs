//! Integration tests for the versioning module.
//!
//! These tests verify that API versioning primitives work correctly when used
//! from outside `typeway-core`, including the `assert_api_compatible!` macro
//! (which is `#[macro_export]` and must be tested from an external crate).

#![allow(non_camel_case_types, dead_code)]

use typeway_core::*;

// ---------------------------------------------------------------------------
// Literal segment markers
// ---------------------------------------------------------------------------

struct users;
impl LitSegment for users {
    const VALUE: &'static str = "users";
}

struct profiles;
impl LitSegment for profiles {
    const VALUE: &'static str = "profiles";
}

struct posts;
impl LitSegment for posts {
    const VALUE: &'static str = "posts";
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct UserV1;
#[derive(Debug)]
struct UserV2;
#[derive(Debug)]
struct CreateUser;
#[derive(Debug)]
struct Profile;
#[derive(Debug)]
struct Post;

// ---------------------------------------------------------------------------
// Path aliases
// ---------------------------------------------------------------------------

type UsersPath = HCons<Lit<users>, HNil>;
type UserByIdPath = HCons<Lit<users>, HCons<Capture<u32>, HNil>>;
type UserProfilePath = HCons<Lit<users>, HCons<Capture<u32>, HCons<Lit<profiles>, HNil>>>;
type UserPostsPath = HCons<Lit<users>, HCons<Capture<u32>, HCons<Lit<posts>, HNil>>>;

// ---------------------------------------------------------------------------
// V1: 3-endpoint API
// ---------------------------------------------------------------------------

type ListUsersV1 = GetEndpoint<UsersPath, Vec<UserV1>>;
type GetUserV1 = GetEndpoint<UserByIdPath, UserV1>;
type CreateUserV1 = PostEndpoint<UsersPath, CreateUser, UserV1>;

type V1 = (ListUsersV1, GetUserV1, CreateUserV1);

// ---------------------------------------------------------------------------
// V2: V1 + one Added + one Replaced
// ---------------------------------------------------------------------------

type V2Changes = (
    Added<GetEndpoint<UserProfilePath, Profile>>,
    Replaced<GetUserV1, GetEndpoint<UserByIdPath, UserV2>>,
    Deprecated<CreateUserV1>,
);

type V2Resolved = (
    ListUsersV1,
    GetEndpoint<UserByIdPath, UserV2>,
    CreateUserV1,
    GetEndpoint<UserProfilePath, Profile>,
);

type V2 = VersionedApi<V1, V2Changes, V2Resolved>;

// ---------------------------------------------------------------------------
// V3: V2 + one more Added
// ---------------------------------------------------------------------------

type V3Changes = (Added<GetEndpoint<UserPostsPath, Vec<Post>>>,);

type V3Resolved = (
    ListUsersV1,
    GetEndpoint<UserByIdPath, UserV2>,
    CreateUserV1,
    GetEndpoint<UserProfilePath, Profile>,
    GetEndpoint<UserPostsPath, Vec<Post>>,
);

type V3 = VersionedApi<V2, V3Changes, V3Resolved>;

// ---------------------------------------------------------------------------
// ApiSpec checks
// ---------------------------------------------------------------------------

fn assert_api_spec<A: ApiSpec>() {}

#[test]
fn v1_implements_api_spec() {
    assert_api_spec::<V1>();
}

#[test]
fn v2_implements_api_spec() {
    assert_api_spec::<V2>();
}

#[test]
fn v3_implements_api_spec() {
    assert_api_spec::<V3>();
}

// ---------------------------------------------------------------------------
// ApiChangelog counts
// ---------------------------------------------------------------------------

#[test]
fn v2_changelog_counts() {
    assert_eq!(<V2Changes as ApiChangelog>::ADDED, 1);
    assert_eq!(<V2Changes as ApiChangelog>::REMOVED, 0);
    assert_eq!(<V2Changes as ApiChangelog>::REPLACED, 1);
    assert_eq!(<V2Changes as ApiChangelog>::DEPRECATED, 1);
}

#[test]
fn v3_changelog_counts() {
    assert_eq!(<V3Changes as ApiChangelog>::ADDED, 1);
    assert_eq!(<V3Changes as ApiChangelog>::REMOVED, 0);
    assert_eq!(<V3Changes as ApiChangelog>::REPLACED, 0);
    assert_eq!(<V3Changes as ApiChangelog>::DEPRECATED, 0);
}

#[test]
fn changelog_with_removal() {
    type Changes = (
        Removed<ListUsersV1>,
        Added<GetEndpoint<UserPostsPath, Vec<Post>>>,
    );
    assert_eq!(<Changes as ApiChangelog>::ADDED, 1);
    assert_eq!(<Changes as ApiChangelog>::REMOVED, 1);
    assert_eq!(<Changes as ApiChangelog>::REPLACED, 0);
    assert_eq!(<Changes as ApiChangelog>::DEPRECATED, 0);
}

#[test]
fn changelog_summary_text() {
    let summary = <V2Changes as ApiChangelog>::summary();
    assert!(summary.contains("Added endpoint"));
    assert!(summary.contains("Replaced endpoint"));
    assert!(summary.contains("Deprecated endpoint"));
}

// ---------------------------------------------------------------------------
// Compile-time backward compatibility checks via assert_api_compatible!
// ---------------------------------------------------------------------------

// V2 preserves ListUsersV1 and CreateUserV1 from V1.
// GetUserV1 was replaced, so we omit it.
assert_api_compatible!((ListUsersV1, CreateUserV1), V2Resolved);

// V3 preserves everything from V2Resolved.
assert_api_compatible!(
    (
        ListUsersV1,
        GetEndpoint<UserByIdPath, UserV2>,
        CreateUserV1,
        GetEndpoint<UserProfilePath, Profile>,
    ),
    V3Resolved
);

// V3 also preserves the non-replaced V1 endpoints transitively.
assert_api_compatible!((ListUsersV1, CreateUserV1), V3Resolved);

// ---------------------------------------------------------------------------
// HasEndpoint direct checks
// ---------------------------------------------------------------------------

#[test]
fn has_endpoint_for_v2_resolved() {
    fn assert_has<Api: HasEndpoint<E, Idx>, E, Idx>() {}

    // All four endpoints in V2Resolved are findable.
    assert_has::<V2Resolved, ListUsersV1, Here>();
    assert_has::<V2Resolved, GetEndpoint<UserByIdPath, UserV2>, _>();
    assert_has::<V2Resolved, CreateUserV1, _>();
    assert_has::<V2Resolved, GetEndpoint<UserProfilePath, Profile>, _>();
}

#[test]
fn has_endpoint_for_v3_resolved() {
    fn assert_has<Api: HasEndpoint<E, Idx>, E, Idx>() {}

    assert_has::<V3Resolved, ListUsersV1, _>();
    assert_has::<V3Resolved, GetEndpoint<UserByIdPath, UserV2>, _>();
    assert_has::<V3Resolved, CreateUserV1, _>();
    assert_has::<V3Resolved, GetEndpoint<UserProfilePath, Profile>, _>();
    assert_has::<V3Resolved, GetEndpoint<UserPostsPath, Vec<Post>>, _>();
}

// ---------------------------------------------------------------------------
// VersionInfo
// ---------------------------------------------------------------------------

struct V1Meta;
impl VersionInfo for V1Meta {
    const VERSION: &'static str = "1.0.0";
    const TITLE: &'static str = "Users API V1";
}

struct V2Meta;
impl VersionInfo for V2Meta {
    const VERSION: &'static str = "2.0.0";
    const TITLE: &'static str = "Users API V2";
}

struct V3Meta;
impl VersionInfo for V3Meta {
    const VERSION: &'static str = "3.0.0";
    const TITLE: &'static str = "Users API V3";
}

#[test]
fn version_info_metadata() {
    assert_eq!(V1Meta::VERSION, "1.0.0");
    assert_eq!(V1Meta::TITLE, "Users API V1");
    assert_eq!(V2Meta::VERSION, "2.0.0");
    assert_eq!(V2Meta::TITLE, "Users API V2");
    assert_eq!(V3Meta::VERSION, "3.0.0");
    assert_eq!(V3Meta::TITLE, "Users API V3");
}

// ---------------------------------------------------------------------------
// Negative test: compile-time failure for missing endpoint
// ---------------------------------------------------------------------------
// The following would NOT compile if uncommented, because GetUserV1 (the
// old endpoint) is not present in V2Resolved (it was replaced by the V2
// variant). This is the intended behavior — the type system catches the
// incompatibility.
//
// assert_api_compatible!((ListUsersV1, GetUserV1, CreateUserV1), V2Resolved);
//
// Similarly, checking for an endpoint type that was never in the API:
// assert_api_compatible!((GetEndpoint<UserPostsPath, Vec<Post>>), V2Resolved);
