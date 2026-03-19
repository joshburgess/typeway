//! `typeway-core` — type-level primitives for the Typeway web framework.
//!
//! This crate provides the foundational type-level machinery: path segment
//! encoding, HTTP method types, endpoint descriptors, and the API spec trait.
//! No I/O, no HTTP, no async — pure type-level Rust.

pub mod api;
pub mod endpoint;
pub mod method;
pub mod path;

pub use api::ApiSpec;
pub use endpoint::{
    DeleteEndpoint, Endpoint, GetEndpoint, NoBody, PatchEndpoint, PostEndpoint, PutEndpoint,
};
pub use method::{Delete, Get, Head, HttpMethod, Options, Patch, Post, Put};
pub use path::{
    Capture, CaptureRest, CapturesPrepend, ExtractPath, HCons, HNil, Lit, LitSegment, PathSpec,
    Prepend,
};
