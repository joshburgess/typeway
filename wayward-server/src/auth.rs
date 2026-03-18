//! Type-level authentication for endpoints.
//!
//! [`Protected<Auth, E>`] wraps an endpoint type to declare that it requires
//! authentication. The compiler enforces that the handler's first argument
//! is the auth extractor type.
//!
//! # Example
//!
//! ```ignore
//! use wayward_server::auth::Protected;
//!
//! // Define your auth extractor (implements FromRequestParts)
//! struct AuthUser(pub Uuid);
//!
//! // Tag endpoints as protected in the API type
//! type API = (
//!     GetEndpoint<TagsPath, TagsResponse>,                           // public
//!     Protected<AuthUser, GetEndpoint<UserPath, UserResponse>>,      // auth required
//!     Protected<AuthUser, PostEndpoint<ArticlesPath, Req, Res>>,     // auth required
//! );
//!
//! // Handlers for protected endpoints MUST accept AuthUser as first arg.
//! // This is enforced at compile time — omitting it is a type error.
//! async fn get_user(auth: AuthUser, state: State<Db>) -> Json<User> { ... }
//! ```
//!
//! `Protected` also informs the OpenAPI generator to mark the endpoint as
//! requiring authentication.

use std::marker::PhantomData;

use wayward_core::{ApiSpec, ExtractPath, HttpMethod, PathSpec};

use crate::extract::FromRequestParts;
use crate::handler::{into_boxed_handler, BoxedHandler, Handler};
use crate::handler_for::{BindableEndpoint, BoundHandler};
use crate::router::Router;

/// An endpoint that requires authentication.
///
/// `Auth` is the authentication extractor type (e.g., `AuthUser`).
/// `E` is the underlying endpoint type.
///
/// When used in an API type, the compiler ensures the handler's first
/// argument is `Auth`. Public endpoints use the bare `Endpoint` type;
/// protected endpoints are wrapped in `Protected`.
pub struct Protected<Auth, E> {
    _marker: PhantomData<(Auth, E)>,
}

// Protected endpoints are valid API specs if the inner endpoint is.
impl<Auth, E: ApiSpec> ApiSpec for Protected<Auth, E> {}

// Protected endpoints are bindable — delegate to the inner endpoint.
impl<Auth, E: BindableEndpoint> BindableEndpoint for Protected<Auth, E> {
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

/// Bind a handler to a protected endpoint.
///
/// The handler's first argument must be `Auth` (the auth extractor).
/// This is enforced by the `Handler<(Auth, ...)>` bound — if the first
/// arg doesn't match, the compiler rejects it.
///
/// ```ignore
/// // This compiles:
/// bind_protected::<AuthUser, GetEndpoint<...>, _, _>(|auth: AuthUser, state: State<Db>| async { ... })
///
/// // This fails to compile — missing AuthUser:
/// bind_protected::<AuthUser, GetEndpoint<...>, _, _>(|state: State<Db>| async { ... })
/// ```
pub fn bind_protected<Auth, E, H, Args>(handler: H) -> BoundHandler<Protected<Auth, E>>
where
    Auth: FromRequestParts + 'static,
    E: BindableEndpoint,
    H: Handler<Args>,
    Args: 'static,
{
    let method = E::method();
    let pattern = E::pattern();
    let match_fn = E::match_fn();

    BoundHandler::new(method, pattern, match_fn, into_boxed_handler(handler))
}

/// Convenience macro for binding protected handlers.
///
/// ```ignore
/// bind_auth!(handler)
/// // equivalent to: bind_protected::<_, _, _, _>(handler)
/// ```
#[macro_export]
macro_rules! bind_auth {
    ($handler:expr) => {
        $crate::auth::bind_protected::<_, _, _, _>($handler)
    };
}
