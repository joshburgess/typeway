//! Type-level endpoint wrappers for compile-time enforcement.
//!
//! These wrappers sit in the API type and enforce constraints at compile time:
//!
//! - [`Validated<V, E>`] — validates request bodies before the handler runs
//! - [`Versioned<V, E>`] — API version routing
//! - [`ContentType<C, E>`] — enforces request content type
//! - [`RateLimited<E>`] — declares rate limiting on an endpoint
//!
//! See also [`Protected<Auth, E>`](crate::auth::Protected) for authentication.

use std::marker::PhantomData;

use typeway_core::ApiSpec;
use typeway_core::effects::{Effect, Requires};

use crate::handler_for::BindableEndpoint;

// ===========================================================================
// Validated<V, E> — request body validation
// ===========================================================================

/// Trait for request body validators.
///
/// Implement this to define validation rules for a request body type.
/// The validator runs after JSON deserialization but before the handler.
///
/// # Example
///
/// ```ignore
/// struct CreateUserValidator;
///
/// impl Validate<CreateUser> for CreateUserValidator {
///     fn validate(body: &CreateUser) -> Result<(), String> {
///         if body.username.is_empty() { return Err("username required".into()); }
///         if body.password.len() < 8 { return Err("password too short".into()); }
///         Ok(())
///     }
/// }
///
/// type API = (
///     Validated<CreateUserValidator, PostEndpoint<UsersPath, CreateUser, User>>,
/// );
/// ```
pub trait Validate<T>: Send + Sync + 'static {
    /// Validate the deserialized body. Returns `Err(message)` on failure.
    fn validate(body: &T) -> Result<(), String>;
}

/// An endpoint with compile-time validated request bodies.
///
/// `V` implements `Validate<Req>` where `Req` is the endpoint's body type.
/// The framework validates the body after deserialization and returns 422
/// if validation fails, before the handler is called.
pub struct Validated<V, E> {
    _marker: PhantomData<(V, E)>,
}

impl<V: Send + Sync + 'static, E: ApiSpec> ApiSpec for Validated<V, E> {}
impl<V: Send + Sync + 'static, E: BindableEndpoint> BindableEndpoint for Validated<V, E> {
    fn method() -> http::Method {
        E::method()
    }
    fn pattern() -> String {
        E::pattern()
    }
    fn match_fn() -> crate::router::MatchFn {
        E::match_fn()
    }
}

// ===========================================================================
// Versioned<V, E> — API versioning
// ===========================================================================

/// A version marker type. Use unit structs for each version.
///
/// ```ignore
/// struct V1;
/// struct V2;
/// ```
pub trait ApiVersion: Send + Sync + 'static {
    /// The version prefix string (e.g., "v1", "v2").
    const PREFIX: &'static str;
}

/// An endpoint scoped to a specific API version.
///
/// The version prefix is prepended to the path at routing time.
///
/// ```ignore
/// struct V1;
/// impl ApiVersion for V1 { const PREFIX: &'static str = "v1"; }
///
/// type API = (
///     Versioned<V1, GetEndpoint<UsersPath, Vec<User>>>,  // /v1/users
///     Versioned<V2, GetEndpoint<UsersPath, Vec<User>>>,  // /v2/users
/// );
/// ```
pub struct Versioned<V, E> {
    _marker: PhantomData<(V, E)>,
}

impl<V: ApiVersion, E: ApiSpec> ApiSpec for Versioned<V, E> {}
impl<V: ApiVersion, E: BindableEndpoint> BindableEndpoint for Versioned<V, E> {
    fn method() -> http::Method {
        E::method()
    }
    fn pattern() -> String {
        format!("/{}{}", V::PREFIX, E::pattern())
    }
    fn match_fn() -> crate::router::MatchFn {
        let prefix = V::PREFIX;
        let inner_match = E::match_fn();
        Box::new(move |segments: &[&str]| {
            if segments.first() == Some(&prefix) {
                inner_match(&segments[1..])
            } else {
                false
            }
        })
    }
}

// ===========================================================================
// ContentType<C, E> — content type enforcement
// ===========================================================================

/// A content type marker.
pub trait ContentTypeMarker: Send + Sync + 'static {
    /// The expected Content-Type header value.
    const CONTENT_TYPE: &'static str;
}

/// Built-in JSON content type marker.
pub struct JsonContent;
impl ContentTypeMarker for JsonContent {
    const CONTENT_TYPE: &'static str = "application/json";
}

/// Built-in form content type marker.
pub struct FormContent;
impl ContentTypeMarker for FormContent {
    const CONTENT_TYPE: &'static str = "application/x-www-form-urlencoded";
}

/// An endpoint that enforces a specific request Content-Type.
///
/// Requests without the correct Content-Type header are rejected with
/// 415 Unsupported Media Type before the handler is called.
///
/// ```ignore
/// type API = (
///     ContentType<JsonContent, PostEndpoint<UsersPath, CreateUser, User>>,
/// );
/// ```
pub struct ContentType<C, E> {
    _marker: PhantomData<(C, E)>,
}

impl<C: ContentTypeMarker, E: ApiSpec> ApiSpec for ContentType<C, E> {}
impl<C: ContentTypeMarker, E: BindableEndpoint> BindableEndpoint for ContentType<C, E> {
    fn method() -> http::Method {
        E::method()
    }
    fn pattern() -> String {
        E::pattern()
    }
    fn match_fn() -> crate::router::MatchFn {
        E::match_fn()
    }
}

// ===========================================================================
// RateLimited<E> — rate limiting declaration
// ===========================================================================

/// Rate limiting configuration.
pub trait RateLimit: Send + Sync + 'static {
    /// Maximum requests per window.
    const MAX_REQUESTS: u32;
    /// Window duration in seconds.
    const WINDOW_SECS: u64;
}

/// An endpoint with declared rate limits.
///
/// This is a type-level declaration — the actual enforcement is done by
/// the framework's rate limiting middleware, which reads the limits from
/// the type at startup.
///
/// ```ignore
/// struct StandardRate;
/// impl RateLimit for StandardRate {
///     const MAX_REQUESTS: u32 = 100;
///     const WINDOW_SECS: u64 = 60;
/// }
///
/// type API = (
///     RateLimited<StandardRate, PostEndpoint<LoginPath, LoginReq, LoginRes>>,
/// );
/// ```
pub struct RateLimited<R, E> {
    _marker: PhantomData<(R, E)>,
}

impl<R: RateLimit, E: ApiSpec> ApiSpec for RateLimited<R, E> {}
impl<R: RateLimit, E: BindableEndpoint> BindableEndpoint for RateLimited<R, E> {
    fn method() -> http::Method {
        E::method()
    }
    fn pattern() -> String {
        E::pattern()
    }
    fn match_fn() -> crate::router::MatchFn {
        E::match_fn()
    }
}

// ===========================================================================
// Requires<E, T> — middleware effect requirement (BindableEndpoint delegation)
// ===========================================================================

/// `Requires<E, T>` delegates all endpoint metadata to the inner type `T`.
///
/// This allows `bind!()` to work with `Requires`-wrapped endpoints.
/// The `Requires` wrapper is purely a compile-time marker — at runtime,
/// routing behaves identically to the unwrapped endpoint.
impl<E: Effect, T: BindableEndpoint> BindableEndpoint for Requires<E, T> {
    fn method() -> http::Method {
        T::method()
    }
    fn pattern() -> String {
        T::pattern()
    }
    fn match_fn() -> crate::router::MatchFn {
        T::match_fn()
    }
}
