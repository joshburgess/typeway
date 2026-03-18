//! The [`Handler`] trait — connects async functions to HTTP routes via extractors.
//!
//! Handler is implemented for async functions of arities 0–16 via macro.
//! Each function argument must implement [`FromRequestParts`] (metadata extractors)
//! or [`FromRequest`] (body extractor, last argument only).

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use crate::body::BoxBody;
use crate::extract::{FromRequest, FromRequestParts};
use crate::response::IntoResponse;

/// A handler that can process an HTTP request and produce a response.
///
/// Implemented automatically for async functions whose arguments are
/// extractors. The `Args` type parameter encodes the extractor tuple
/// and is used for trait resolution — users don't specify it directly.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a valid handler for arguments `{Args}`",
    label = "not a valid handler",
    note = "handlers must be async functions whose arguments implement `FromRequestParts` or `FromRequest`"
)]
pub trait Handler<Args>: Clone + Send + Sync + 'static {
    /// Call the handler with the given request parts and pre-collected body bytes.
    fn call(
        self,
        parts: http::request::Parts,
        body: bytes::Bytes,
    ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>>;
}

// ---------------------------------------------------------------------------
// Arity 0: fn() -> Res
// ---------------------------------------------------------------------------

impl<F, Fut, Res> Handler<()> for F
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

// ---------------------------------------------------------------------------
// FromRequestParts-only arities (1–8)
// ---------------------------------------------------------------------------

macro_rules! impl_handler_from_parts {
    ([$($T:ident),+], [$($t:ident),+]) => {
        #[allow(non_snake_case)]
        impl<F, Fut, Res, $($T,)+> Handler<($($T,)+)> for F
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

impl_handler_from_parts!([T1], [t1]);
impl_handler_from_parts!([T1, T2], [t1, t2]);
impl_handler_from_parts!([T1, T2, T3], [t1, t2, t3]);
impl_handler_from_parts!([T1, T2, T3, T4], [t1, t2, t3, t4]);
impl_handler_from_parts!([T1, T2, T3, T4, T5], [t1, t2, t3, t4, t5]);
impl_handler_from_parts!([T1, T2, T3, T4, T5, T6], [t1, t2, t3, t4, t5, t6]);
impl_handler_from_parts!([T1, T2, T3, T4, T5, T6, T7], [t1, t2, t3, t4, t5, t6, t7]);
impl_handler_from_parts!(
    [T1, T2, T3, T4, T5, T6, T7, T8],
    [t1, t2, t3, t4, t5, t6, t7, t8]
);

// ---------------------------------------------------------------------------
// Mixed: FromRequestParts args + one FromRequest body (last arg)
// ---------------------------------------------------------------------------

/// Marker type to distinguish "parts extractors + body extractor" from
/// "all parts extractors" in Handler impls.
pub struct WithBody<Parts, Body>(PhantomData<(Parts, Body)>);

macro_rules! impl_handler_with_body {
    // Special case: only a body extractor, no parts extractors
    ([], []) => {
        impl<F, Fut, Res, B> Handler<WithBody<(), B>> for F
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
    };
    ([$($T:ident),+], [$($t:ident),+]) => {
        #[allow(non_snake_case)]
        impl<F, Fut, Res, $($T,)+ B> Handler<WithBody<($($T,)+), B>> for F
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

impl_handler_with_body!([], []);
impl_handler_with_body!([T1], [t1]);
impl_handler_with_body!([T1, T2], [t1, t2]);
impl_handler_with_body!([T1, T2, T3], [t1, t2, t3]);
impl_handler_with_body!([T1, T2, T3, T4], [t1, t2, t3, t4]);
impl_handler_with_body!([T1, T2, T3, T4, T5], [t1, t2, t3, t4, t5]);
impl_handler_with_body!([T1, T2, T3, T4, T5, T6], [t1, t2, t3, t4, t5, t6]);
impl_handler_with_body!([T1, T2, T3, T4, T5, T6, T7], [t1, t2, t3, t4, t5, t6, t7]);

// ---------------------------------------------------------------------------
// Type-erased handler for storage in the router
// ---------------------------------------------------------------------------

/// A pinned, boxed future producing an HTTP response.
pub type ResponseFuture = Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>>;

/// A type-erased handler stored in the router.
///
/// Handlers receive pre-collected body bytes. This enables both Hyper
/// and Axum body types to be collected at the router boundary before
/// dispatch, avoiding body-type coupling in the handler infrastructure.
pub type BoxedHandler =
    Box<dyn Fn(http::request::Parts, bytes::Bytes) -> ResponseFuture + Send + Sync>;

/// Erase a handler's type for storage in the router.
pub fn into_boxed_handler<H, Args>(handler: H) -> BoxedHandler
where
    H: Handler<Args>,
    Args: 'static,
{
    Box::new(move |parts, body| {
        let h = handler.clone();
        h.call(parts, body)
    })
}
