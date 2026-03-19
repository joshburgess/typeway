//! Bridge between typed handlers and the runtime router.
//!
//! [`BoundHandler<E>`] pairs a type-erased handler with its endpoint metadata.
//! The [`bind`] function captures the handler's `Args` at the call site (where
//! type inference works) and erases them, leaving only the endpoint type `E`.

use std::marker::PhantomData;

use typeway_core::{Endpoint, ExtractPath, HttpMethod, PathSpec};

use crate::handler::{into_boxed_handler, BoxedHandler, Handler};
use crate::router::Router;

/// A handler bound to a specific endpoint type, with the handler's arg types erased.
///
/// Created via the [`bind`] free function. The endpoint type `E` is preserved
/// for compile-time API completeness checks via [`Serves`](crate::serves::Serves).
pub struct BoundHandler<E> {
    method: http::Method,
    pattern: String,
    match_fn: crate::router::MatchFn,
    handler: BoxedHandler,
    _endpoint: PhantomData<E>,
}

impl<E> BoundHandler<E> {
    /// Create a new bound handler with explicit endpoint metadata.
    pub fn new(
        method: http::Method,
        pattern: String,
        match_fn: crate::router::MatchFn,
        handler: BoxedHandler,
    ) -> Self {
        BoundHandler {
            method,
            pattern,
            match_fn,
            handler,
            _endpoint: PhantomData,
        }
    }

    /// Register this handler into the router.
    pub(crate) fn register_into(self, router: &mut Router) {
        router.add_route(self.method, self.pattern, self.match_fn, self.handler);
    }
}

/// Bind a handler to an endpoint type, erasing the handler's arg types.
///
/// This is where type inference resolves the handler's `Args` parameter.
/// After binding, only the endpoint type `E` remains.
///
/// # Example
///
/// ```ignore
/// use typeway_server::bind;
///
/// let h = bind::<GetEndpoint<path!("hello"), String>, _, _>(hello_handler);
/// ```
pub fn bind<E, H, Args>(handler: H) -> BoundHandler<E>
where
    E: BindableEndpoint,
    H: Handler<Args>,
    Args: 'static,
{
    let method = E::method();
    let pattern = E::pattern();
    let match_fn = E::match_fn();

    BoundHandler {
        method,
        pattern,
        match_fn,
        handler: into_boxed_handler(handler),
        _endpoint: PhantomData,
    }
}

/// Convenience macro to bind a handler to an endpoint without turbofish.
///
/// Instead of `bind::<_, _, _>(handler)`, write `bind!(handler)`.
/// The endpoint type is inferred from the `Serves<API>` context.
///
/// ```ignore
/// Server::<API>::new((
///     bind!(hello),
///     bind!(get_user),
///     bind!(create_user),
/// ));
/// ```
#[macro_export]
macro_rules! bind {
    ($handler:expr) => {
        $crate::bind::<_, _, _>($handler)
    };
}

/// Trait providing runtime endpoint metadata for binding.
pub trait BindableEndpoint {
    fn method() -> http::Method;
    fn pattern() -> String;
    fn match_fn() -> crate::router::MatchFn;
}

impl<M, P, Req, Res, Q, Err> BindableEndpoint for Endpoint<M, P, Req, Res, Q, Err>
where
    M: HttpMethod,
    P: PathSpec + ExtractPath + Send + 'static,
    P::Captures: Send,
{
    fn method() -> http::Method {
        M::METHOD
    }

    fn pattern() -> String {
        P::pattern()
    }

    fn match_fn() -> crate::router::MatchFn {
        Box::new(|segments| P::extract(segments).is_some())
    }
}
