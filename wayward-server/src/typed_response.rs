//! Typed response wrapper for compile-time return type enforcement.
//!
//! [`Strict<E>`] wraps an endpoint to enforce that the handler returns
//! the exact type declared in the API spec. Without `Strict`, handlers
//! can return any `impl IntoResponse` regardless of the declared `Res`.
//!
//! # Example
//!
//! ```ignore
//! type API = (
//!     // Handler MUST return Json<Vec<User>> — compiler enforced
//!     Strict<GetEndpoint<UsersPath, Json<Vec<User>>>>,
//!
//!     // Handler can return anything implementing IntoResponse
//!     GetEndpoint<TagsPath, TagsResponse>,
//! );
//!
//! // This compiles — return type matches Res:
//! async fn list_users() -> Json<Vec<User>> { Json(vec![]) }
//!
//! // This would NOT compile — String ≠ Json<Vec<User>>:
//! // async fn list_users() -> String { "nope".into() }
//! ```

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use wayward_core::ApiSpec;

use crate::body::BoxBody;
use crate::extract::{FromRequest, FromRequestParts};
use crate::handler::BoxedHandler;
use crate::handler_for::{BindableEndpoint, BoundHandler};
use crate::response::IntoResponse;

/// An endpoint that enforces the handler's return type matches `Res`.
///
/// `BindableEndpoint` is NOT implemented — `bind!()` cannot be used.
/// Use `bind_strict!()` instead, which checks the return type.
pub struct Strict<E> {
    _marker: PhantomData<E>,
}

impl<E: ApiSpec> ApiSpec for Strict<E> {}

// NOTE: BindableEndpoint intentionally NOT implemented for Strict.
// This forces users to use bind_strict!() which checks the return type.

/// Trait to extract the Res type from an endpoint.
pub trait HasResType {
    type Res;
}

impl<M: wayward_core::HttpMethod, P: wayward_core::PathSpec, Req, Res, Q, Err> HasResType
    for wayward_core::Endpoint<M, P, Req, Res, Q, Err>
{
    type Res = Res;
}

/// Trait to extract the Err type from an endpoint.
pub trait HasErrType {
    type Err;
}

impl<M: wayward_core::HttpMethod, P: wayward_core::PathSpec, Req, Res, Q, Err> HasErrType
    for wayward_core::Endpoint<M, P, Req, Res, Q, Err>
{
    type Err = Err;
}

/// A handler that returns exactly type `Res`.
pub trait StrictHandler<Res, Args>: Clone + Send + Sync + 'static {
    fn call(
        self,
        parts: http::request::Parts,
        body: bytes::Bytes,
    ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>>;
}

// Arity 0: fn() -> Res
impl<F, Fut, Res> StrictHandler<Res, ()> for F
where
    F: FnOnce() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Res> + Send,
    Res: IntoResponse,
{
    fn call(
        self,
        _parts: http::request::Parts,
        _body: bytes::Bytes,
    ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
        Box::pin(async move { self().await.into_response() })
    }
}

// Generate impls for fn(extractors...) -> Res
macro_rules! impl_strict_handler {
    ([$($T:ident),+], [$($t:ident),+]) => {
        #[allow(non_snake_case)]
        impl<F, Fut, Res, $($T,)+> StrictHandler<Res, ($($T,)+)> for F
        where
            F: FnOnce($($T,)+) -> Fut + Clone + Send + Sync + 'static,
            Fut: Future<Output = Res> + Send,
            Res: IntoResponse,
            $($T: FromRequestParts + 'static,)+
        {
            fn call(
                self,
                parts: http::request::Parts,
                _body: bytes::Bytes,
            ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
                Box::pin(async move {
                    $(
                        let $t = match $T::from_request_parts(&parts) {
                            Ok(v) => v,
                            Err(e) => return e.into_response(),
                        };
                    )+
                    self($($t,)+).await.into_response()
                })
            }
        }
    };
}

impl_strict_handler!([T1], [t1]);
impl_strict_handler!([T1, T2], [t1, t2]);
impl_strict_handler!([T1, T2, T3], [t1, t2, t3]);
impl_strict_handler!([T1, T2, T3, T4], [t1, t2, t3, t4]);
impl_strict_handler!([T1, T2, T3, T4, T5], [t1, t2, t3, t4, t5]);
impl_strict_handler!([T1, T2, T3, T4, T5, T6], [t1, t2, t3, t4, t5, t6]);

/// Marker for strict handlers with a body extractor.
pub struct StrictWithBody<Parts, Body>(PhantomData<(Parts, Body)>);

impl<F, Fut, Res, B> StrictHandler<Res, StrictWithBody<(), B>> for F
where
    F: FnOnce(B) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Res> + Send,
    Res: IntoResponse,
    B: FromRequest + 'static,
{
    fn call(
        self,
        parts: http::request::Parts,
        body: bytes::Bytes,
    ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
        Box::pin(async move {
            let b = match B::from_request(&parts, body).await {
                Ok(v) => v,
                Err(e) => return e.into_response(),
            };
            self(b).await.into_response()
        })
    }
}

macro_rules! impl_strict_handler_body_marker {
    ([$($T:ident),+], [$($t:ident),+]) => {
        #[allow(non_snake_case)]
        impl<F, Fut, Res, $($T,)+ B> StrictHandler<Res, StrictWithBody<($($T,)+), B>> for F
        where
            F: FnOnce($($T,)+ B) -> Fut + Clone + Send + Sync + 'static,
            Fut: Future<Output = Res> + Send,
            Res: IntoResponse,
            $($T: FromRequestParts + 'static,)+
            B: FromRequest + 'static,
        {
            fn call(
                self,
                parts: http::request::Parts,
                body: bytes::Bytes,
            ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
                Box::pin(async move {
                    $(
                        let $t = match $T::from_request_parts(&parts) {
                            Ok(v) => v,
                            Err(e) => return e.into_response(),
                        };
                    )+
                    let b = match B::from_request(&parts, body).await {
                        Ok(v) => v,
                        Err(e) => return e.into_response(),
                    };
                    self($($t,)+ b).await.into_response()
                })
            }
        }
    };
}

impl_strict_handler_body_marker!([T1], [t1]);
impl_strict_handler_body_marker!([T1, T2], [t1, t2]);
impl_strict_handler_body_marker!([T1, T2, T3], [t1, t2, t3]);
impl_strict_handler_body_marker!([T1, T2, T3, T4], [t1, t2, t3, t4]);
impl_strict_handler_body_marker!([T1, T2, T3, T4, T5], [t1, t2, t3, t4, t5]);

// ---------------------------------------------------------------------------
// bind_strict
// ---------------------------------------------------------------------------

/// Trait to extract binding info from a Strict endpoint.
pub trait StrictEndpoint {
    type Inner: BindableEndpoint + HasResType;
}

impl<E: BindableEndpoint + HasResType> StrictEndpoint for Strict<E> {
    type Inner = E;
}

/// Bind a handler to a `Strict<E>` endpoint.
///
/// The handler's return type must exactly match `E::Res`.
pub fn bind_strict<S, H, Args>(handler: H) -> BoundHandler<S>
where
    S: StrictEndpoint,
    H: StrictHandler<<S::Inner as HasResType>::Res, Args>,
    Args: 'static,
{
    let method = S::Inner::method();
    let pattern = S::Inner::pattern();
    let match_fn = S::Inner::match_fn();

    let boxed: BoxedHandler = Box::new(move |parts, body| {
        let h = handler.clone();
        h.call(parts, body)
    });

    BoundHandler::new(method, pattern, match_fn, boxed)
}

/// Convenience macro for binding strict handlers.
#[macro_export]
macro_rules! bind_strict {
    ($handler:expr) => {
        $crate::typed_response::bind_strict::<_, _, _>($handler)
    };
}
