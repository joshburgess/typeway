//! Type-level authentication for endpoints.
//!
//! [`Protected<Auth, E>`] wraps an endpoint type to declare that it requires
//! authentication. The compiler enforces that the handler's first argument
//! is the auth extractor type.
//!
//! # Example
//!
//! ```ignore
//! use typeway_server::auth::Protected;
//!
//! // Tag endpoints as protected in the API type
//! type API = (
//!     GetEndpoint<TagsPath, TagsResponse>,                           // public
//!     Protected<AuthUser, GetEndpoint<UserPath, UserResponse>>,      // auth required
//! );
//!
//! // Handlers for protected endpoints MUST accept AuthUser as first arg.
//! async fn get_user(auth: AuthUser, state: State<Db>) -> Json<User> { ... }
//!
//! // Wire up with bind_auth!():
//! Server::<API>::new((
//!     bind!(get_tags),             // public
//!     bind_auth!(get_user),        // protected — AuthUser enforced
//! ));
//! ```

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use typeway_core::ApiSpec;

use crate::body::BoxBody;
use crate::extract::{FromRequest, FromRequestParts};
use crate::handler::BoxedHandler;
use crate::handler_for::{BindableEndpoint, BoundHandler};
use crate::response::IntoResponse;

/// An endpoint that requires authentication.
///
/// `Auth` is the authentication extractor type (e.g., `AuthUser`).
/// `E` is the underlying endpoint type.
///
/// Handlers bound to `Protected` endpoints via `bind_auth!()` must accept
/// `Auth` as their first argument. This is enforced at compile time by
/// the `AuthHandler` trait — using `bind!()` (without auth) for a
/// `Protected` endpoint produces a type mismatch.
pub struct Protected<Auth, E> {
    _marker: PhantomData<(Auth, E)>,
}

impl<Auth, E: ApiSpec> ApiSpec for Protected<Auth, E> {}

// Protected<Auth, E> delegates AllProvided to the inner endpoint E.
// This allows EffectfulServer to work with APIs containing Protected endpoints.
impl<Auth, E, Provided, Idx> typeway_core::effects::AllProvided<Provided, Idx>
    for Protected<Auth, E>
where
    E: typeway_core::effects::AllProvided<Provided, Idx>,
{
}

// NOTE: BindableEndpoint is intentionally NOT implemented for Protected.
// This means bind!() cannot be used with Protected endpoints — only
// bind_auth!() works. This is the compile-time enforcement mechanism.

// ---------------------------------------------------------------------------
// AuthHandler trait — enforces Auth as first argument
// ---------------------------------------------------------------------------

/// A handler that takes `Auth` as its first argument.
///
/// This is separate from `Handler<Args>` to ensure that `Protected`
/// endpoints can only be bound with handlers that accept the auth type.
/// The trait is implemented for async functions where the first argument
/// is `Auth: FromRequestParts`.
pub trait AuthHandler<Auth, Args>: Clone + Send + Sync + 'static {
    fn call(
        self,
        parts: http::request::Parts,
        body: bytes::Bytes,
    ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>>;
}

// Auth + no other args
impl<F, Fut, Res, Auth> AuthHandler<Auth, ()> for F
where
    F: FnOnce(Auth) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Res> + Send,
    Res: IntoResponse,
    Auth: FromRequestParts + 'static,
{
    fn call(
        self,
        parts: http::request::Parts,
        _body: bytes::Bytes,
    ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
        Box::pin(async move {
            let auth = match Auth::from_request_parts(&parts) {
                Ok(v) => v,
                Err(e) => return e.into_response(),
            };
            self(auth).await.into_response()
        })
    }
}

// Generate impls for Auth + N FromRequestParts args
macro_rules! impl_auth_handler_parts {
    ([$($T:ident),+], [$($t:ident),+]) => {
        #[allow(non_snake_case)]
        impl<F, Fut, Res, Auth, $($T,)+> AuthHandler<Auth, ($($T,)+)> for F
        where
            F: FnOnce(Auth, $($T,)+) -> Fut + Clone + Send + Sync + 'static,
            Fut: Future<Output = Res> + Send,
            Res: IntoResponse,
            Auth: FromRequestParts + 'static,
            $($T: FromRequestParts + 'static,)+
        {
            fn call(
                self,
                parts: http::request::Parts,
                _body: bytes::Bytes,
            ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
                Box::pin(async move {
                    let auth = match Auth::from_request_parts(&parts) {
                        Ok(v) => v,
                        Err(e) => return e.into_response(),
                    };
                    $(
                        let $t = match $T::from_request_parts(&parts) {
                            Ok(v) => v,
                            Err(e) => return e.into_response(),
                        };
                    )+
                    self(auth, $($t,)+).await.into_response()
                })
            }
        }
    };
}

impl_auth_handler_parts!([T1], [t1]);
impl_auth_handler_parts!([T1, T2], [t1, t2]);
impl_auth_handler_parts!([T1, T2, T3], [t1, t2, t3]);
impl_auth_handler_parts!([T1, T2, T3, T4], [t1, t2, t3, t4]);
impl_auth_handler_parts!([T1, T2, T3, T4, T5], [t1, t2, t3, t4, t5]);
impl_auth_handler_parts!([T1, T2, T3, T4, T5, T6], [t1, t2, t3, t4, t5, t6]);

// Generate impls for Auth + N FromRequestParts args + body extractor (last arg)
macro_rules! impl_auth_handler_with_body {
    ([], []) => {
        impl<F, Fut, Res, Auth, B> AuthHandler<Auth, AuthWithBody<(), B>> for F
        where
            F: FnOnce(Auth, B) -> Fut + Clone + Send + Sync + 'static,
            Fut: Future<Output = Res> + Send,
            Res: IntoResponse,
            Auth: FromRequestParts + 'static,
            B: FromRequest + 'static,
        {
            fn call(
                self,
                parts: http::request::Parts,
                body: bytes::Bytes,
            ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
                Box::pin(async move {
                    let auth = match Auth::from_request_parts(&parts) {
                        Ok(v) => v,
                        Err(e) => return e.into_response(),
                    };
                    let b = match B::from_request(&parts, body).await {
                        Ok(v) => v,
                        Err(e) => return e.into_response(),
                    };
                    self(auth, b).await.into_response()
                })
            }
        }
    };
    ([$($T:ident),+], [$($t:ident),+]) => {
        #[allow(non_snake_case)]
        impl<F, Fut, Res, Auth, $($T,)+ B> AuthHandler<Auth, AuthWithBody<($($T,)+), B>> for F
        where
            F: FnOnce(Auth, $($T,)+ B) -> Fut + Clone + Send + Sync + 'static,
            Fut: Future<Output = Res> + Send,
            Res: IntoResponse,
            Auth: FromRequestParts + 'static,
            $($T: FromRequestParts + 'static,)+
            B: FromRequest + 'static,
        {
            fn call(
                self,
                parts: http::request::Parts,
                body: bytes::Bytes,
            ) -> Pin<Box<dyn Future<Output = http::Response<BoxBody>> + Send>> {
                Box::pin(async move {
                    let auth = match Auth::from_request_parts(&parts) {
                        Ok(v) => v,
                        Err(e) => return e.into_response(),
                    };
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
                    self(auth, $($t,)+ b).await.into_response()
                })
            }
        }
    };
}

/// Marker for auth handlers with a body extractor as last arg.
pub struct AuthWithBody<Parts, Body>(PhantomData<(Parts, Body)>);

impl_auth_handler_with_body!([], []);
impl_auth_handler_with_body!([T1], [t1]);
impl_auth_handler_with_body!([T1, T2], [t1, t2]);
impl_auth_handler_with_body!([T1, T2, T3], [t1, t2, t3]);
impl_auth_handler_with_body!([T1, T2, T3, T4], [t1, t2, t3, t4]);
impl_auth_handler_with_body!([T1, T2, T3, T4, T5], [t1, t2, t3, t4, t5]);

// ---------------------------------------------------------------------------
// bind_protected — uses AuthHandler instead of Handler
// ---------------------------------------------------------------------------

/// Bind a handler to a `Protected<Auth, E>` endpoint.
///
/// The handler's first argument MUST be `Auth`. This is enforced by the
/// `AuthHandler<Auth, Args>` trait — the compiler rejects handlers that
/// don't take `Auth` as their first argument.
/// Trait to extract binding info from the inner endpoint of a Protected type.
pub trait ProtectedEndpoint {
    type Auth;
    type Inner: BindableEndpoint;
}

impl<Auth, E: BindableEndpoint> ProtectedEndpoint for Protected<Auth, E> {
    type Auth = Auth;
    type Inner = E;
}

pub fn bind_protected<P, H, Args>(handler: H) -> BoundHandler<P>
where
    P: ProtectedEndpoint,
    P::Auth: FromRequestParts + 'static,
    P::Inner: BindableEndpoint,
    H: AuthHandler<P::Auth, Args>,
    Args: 'static,
{
    let method = P::Inner::method();
    let pattern = P::Inner::pattern();
    let match_fn = P::Inner::match_fn();

    // Type-erase via AuthHandler::call
    let boxed: BoxedHandler = Box::new(move |parts, body| {
        let h = handler.clone();
        h.call(parts, body)
    });

    BoundHandler::new(method, pattern, match_fn, boxed)
}

/// Convenience macro for binding protected handlers.
#[macro_export]
macro_rules! bind_auth {
    ($handler:expr) => {
        $crate::auth::bind_protected::<_, _, _>($handler)
    };
}
