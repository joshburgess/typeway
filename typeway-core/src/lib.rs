//! `typeway-core` — type-level primitives for the Typeway web framework.
//!
//! This crate provides the foundational type-level machinery: path segment
//! encoding, HTTP method types, endpoint descriptors, and the API spec trait.
//! No I/O, no HTTP, no async — pure type-level Rust.

pub mod api;
pub mod effects;
pub mod endpoint;
pub mod method;
pub mod negotiate;
pub mod path;
pub mod session;
pub mod versioning;

pub use api::ApiSpec;
pub use effects::{
    AllProvided, AuthRequired, CorsRequired, ECons, EHere, ENil, EThere, Effect, HasEffect,
    RateLimitRequired, Requires, TracingRequired,
};
pub use endpoint::{
    DeleteEndpoint, Endpoint, GetEndpoint, NoBody, PatchEndpoint, PostEndpoint, PutEndpoint,
};
pub use method::{Delete, Get, Head, HttpMethod, Options, Patch, Post, Put};
pub use negotiate::{ContentFormat, Negotiated};
pub use path::{
    Capture, CaptureRest, CapturesPrepend, ExtractPath, HCons, HNil, Lit, LitSegment, PathSpec,
    Prepend,
};
// Session types are accessed via `typeway_core::session::*` to avoid
// shadowing `std::marker::Send` when using `typeway_core::*`.
pub use session::{Dual, End, Offer, Rec, Recv, Select, SessionType, Var};
pub use versioning::{
    Added, ApiChangelog, VersionedApi, BackwardCompatible, ChangeMarker, Deprecated, HasEndpoint,
    Here, IsEndpoint, Removed, Replaced, There, VersionInfo,
};
