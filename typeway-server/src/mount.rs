//! Builder-style API composition with `.mount()`.
//!
//! For APIs larger than 25 endpoints, or when you want to compose
//! sub-APIs incrementally, the mount builder provides a flat,
//! readable alternative to nested tuples.
//!
//! # Example
//!
//! ```ignore
//! type UsersAPI = (GetEndpoint<UsersPath, Vec<User>>, PostEndpoint<UsersPath, CreateUser, User>);
//! type OrdersAPI = (GetEndpoint<OrdersPath, Vec<Order>>,);
//! type FullAPI = (UsersAPI, OrdersAPI);
//!
//! ServerBuilder::<FullAPI>::new()
//!     .mount::<UsersAPI>((bind!(list_users), bind!(create_user)))
//!     .mount::<OrdersAPI>((bind!(list_orders),))
//!     .build()
//!     .serve("0.0.0.0:3000".parse()?)
//!     .await?;
//! ```
//!
//! Each `.mount()` call registers a sub-API's handlers into the router.
//! `.build()` requires all sub-APIs in the full API type to have been
//! mounted — missing a mount is a compile error.

use std::convert::Infallible;
use std::marker::PhantomData;
use std::sync::Arc;

use typeway_core::effects::{AllProvided, ECons, ENil, Effect};
use typeway_core::ApiSpec;

use crate::body::BoxBody;
use crate::router::{Router, RouterService};
use crate::serves::Serves;

// ---------------------------------------------------------------------------
// Type-level mount tracking
// ---------------------------------------------------------------------------

/// Type-level empty list: no sub-APIs mounted yet.
pub struct MNil;

/// Type-level cons: sub-API `A` has been mounted, followed by `Tail`.
pub struct MCons<A, Tail>(PhantomData<(A, Tail)>);

/// Type-level proof that a sub-API has been mounted.
///
/// Uses the same index witness technique as the effects system.
pub struct MHere;

/// Type-level index: the sub-API is somewhere later in the list.
pub struct MThere<T>(PhantomData<T>);

/// Asserts that sub-API `A` is in the mounted list `M`.
pub trait HasMount<A, Idx> {}

impl<A, Tail> HasMount<A, MHere> for MCons<A, Tail> {}

impl<A, Head, Tail, Idx> HasMount<A, MThere<Idx>> for MCons<Head, Tail> where Tail: HasMount<A, Idx> {}

/// Asserts that ALL sub-APIs in `FullAPI` have been mounted.
///
/// Works like `AllProvided` for effects — recursively checks each
/// element of the API tuple against the mounted list.
#[diagnostic::on_unimplemented(
    message = "not all sub-APIs have been mounted for `{Self}`",
    label = "some sub-APIs are missing — add more .mount() calls",
    note = "each sub-API in the API type must have a corresponding .mount() call"
)]
pub trait AllMounted<M, Idx> {}

// Unit tuple — nothing to mount.
impl<M> AllMounted<M, ()> for () {}

// Tuples: each element must be present in the mounted list.
macro_rules! impl_all_mounted_for_tuple {
    ($($T:ident, $I:ident);+) => {
        impl<Mounted, $($T: ApiSpec, $I,)+> AllMounted<Mounted, ($($I,)+)> for ($($T,)+)
        where $(Mounted: HasMount<$T, $I>,)+ {}
    };
}

impl_all_mounted_for_tuple!(A, IA);
impl_all_mounted_for_tuple!(A, IA; B, IB);
impl_all_mounted_for_tuple!(A, IA; B, IB; C, IC);
impl_all_mounted_for_tuple!(A, IA; B, IB; C, IC; D, ID);
impl_all_mounted_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE);
impl_all_mounted_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF);
impl_all_mounted_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG);
impl_all_mounted_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH);
impl_all_mounted_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II);
impl_all_mounted_for_tuple!(A, IA; B, IB; C, IC; D, ID; E, IE; F, IF; G, IG; H, IH; I, II; J, IJ);

// ---------------------------------------------------------------------------
// ServerBuilder
// ---------------------------------------------------------------------------

/// A builder for composing large APIs from sub-APIs.
///
/// Each `.mount::<SubAPI>(handlers)` call registers a sub-API's handlers
/// and records the mount at the type level. `.build()` only compiles
/// when all sub-APIs in the full API type have been mounted.
///
/// # Example
///
/// ```ignore
/// ServerBuilder::<FullAPI>::new()
///     .mount::<UsersAPI>(users_handlers)
///     .mount::<OrdersAPI>(orders_handlers)
///     .build()
///     .serve(addr)
///     .await?;
/// ```
/// A builder for composing large APIs from sub-APIs with compile-time
/// tracking of both mounted sub-APIs and provided middleware effects.
///
/// - `A`: the full API type
/// - `Mounted`: type-level list of mounted sub-APIs (starts as `MNil`)
/// - `Provided`: type-level list of provided effects (starts as `ENil`)
pub struct ServerBuilder<A: ApiSpec, Mounted = MNil, Provided = ENil> {
    router: Router,
    _api: PhantomData<A>,
    _mounted: PhantomData<Mounted>,
    _provided: PhantomData<Provided>,
}

impl<A: ApiSpec> ServerBuilder<A, MNil, ENil> {
    /// Create a new builder for the given API type.
    pub fn new() -> Self {
        ServerBuilder {
            router: Router::new(),
            _api: PhantomData,
            _mounted: PhantomData,
            _provided: PhantomData,
        }
    }
}

impl<A: ApiSpec, M, P> ServerBuilder<A, M, P> {
    /// Mount a sub-API with its handler tuple.
    ///
    /// The type system tracks which sub-APIs have been mounted.
    pub fn mount<Sub: ApiSpec, H: Serves<Sub>>(
        mut self,
        handlers: H,
    ) -> ServerBuilder<A, MCons<Sub, M>, P> {
        handlers.register(&mut self.router);
        ServerBuilder {
            router: self.router,
            _api: PhantomData,
            _mounted: PhantomData,
            _provided: PhantomData,
        }
    }

    /// Declare that a middleware effect has been provided.
    ///
    /// Pair with `.layer()` to apply the actual middleware. The type
    /// system tracks which effects have been provided across all sub-APIs.
    pub fn provide<E: Effect>(self) -> ServerBuilder<A, M, ECons<E, P>> {
        ServerBuilder {
            router: self.router,
            _api: PhantomData,
            _mounted: PhantomData,
            _provided: PhantomData,
        }
    }

    /// Apply a Tower middleware layer to the server.
    ///
    /// Typically paired with `.provide::<E>()` to discharge an effect
    /// requirement. The layer wraps the entire router — applied once,
    /// covering all mounted sub-APIs.
    pub fn layer<L>(self, layer: L) -> LayeredServerBuilder<A, M, P, L::Service>
    where
        L: tower_layer::Layer<RouterService>,
        L::Service: tower_service::Service<
                http::Request<hyper::body::Incoming>,
                Response = http::Response<BoxBody>,
                Error = Infallible,
            > + Clone
            + Send
            + 'static,
        <L::Service as tower_service::Service<http::Request<hyper::body::Incoming>>>::Future:
            Send + 'static,
    {
        let router = Arc::new(self.router);
        let svc = RouterService::new(router.clone());
        let layered = layer.layer(svc);
        LayeredServerBuilder {
            service: layered,
            router,
            _api: PhantomData,
            _mounted: PhantomData,
            _provided: PhantomData,
        }
    }

    /// Set shared state accessible via `State<T>` extractors.
    pub fn with_state<T: Clone + Send + Sync + 'static>(self, state: T) -> Self {
        self.router.set_state_injector(Arc::new(move |ext| {
            ext.insert(state.clone());
        }));
        self
    }

    /// Set the maximum request body size in bytes.
    pub fn max_body_size(self, max: usize) -> Self {
        self.router.set_max_body_size(max);
        self
    }

    /// Finalize the server.
    ///
    /// Only compiles when:
    /// - All sub-APIs in the full API type have been mounted
    /// - All `Requires<E, _>` effects have been provided
    pub fn build<MIdx, PIdx>(self) -> crate::server::Server<A>
    where
        A: AllMounted<M, MIdx>,
        A: AllProvided<P, PIdx>,
    {
        crate::server::Server::from_router(Arc::new(self.router))
    }
}

/// A [`ServerBuilder`] with Tower middleware layers applied.
///
/// Created by [`ServerBuilder::layer`]. Supports further `.provide()`,
/// `.layer()`, and `.build()` calls.
pub struct LayeredServerBuilder<A: ApiSpec, Mounted, Provided, S> {
    service: S,
    router: Arc<Router>,
    _api: PhantomData<A>,
    _mounted: PhantomData<Mounted>,
    _provided: PhantomData<Provided>,
}

impl<A: ApiSpec, M, P, S> LayeredServerBuilder<A, M, P, S> {
    /// Declare that a middleware effect has been provided.
    pub fn provide<E: Effect>(self) -> LayeredServerBuilder<A, M, ECons<E, P>, S> {
        LayeredServerBuilder {
            service: self.service,
            router: self.router,
            _api: PhantomData,
            _mounted: PhantomData,
            _provided: PhantomData,
        }
    }
}

impl<A: ApiSpec, M, P, S> LayeredServerBuilder<A, M, P, S>
where
    S: tower_service::Service<
            http::Request<hyper::body::Incoming>,
            Response = http::Response<BoxBody>,
            Error = Infallible,
        > + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    /// Finalize the layered server.
    ///
    /// Only compiles when all sub-APIs are mounted and all effects provided.
    pub fn build<MIdx, PIdx>(self) -> crate::server::LayeredServer<S>
    where
        A: AllMounted<M, MIdx>,
        A: AllProvided<P, PIdx>,
    {
        crate::server::LayeredServer {
            service: self.service,
            router: self.router,
        }
    }

    /// Apply another Tower middleware layer.
    pub fn layer<L>(self, layer: L) -> LayeredServerBuilder<A, M, P, L::Service>
    where
        L: tower_layer::Layer<S>,
        L::Service: tower_service::Service<
                http::Request<hyper::body::Incoming>,
                Response = http::Response<BoxBody>,
                Error = Infallible,
            > + Clone
            + Send
            + 'static,
        <L::Service as tower_service::Service<http::Request<hyper::body::Incoming>>>::Future:
            Send + 'static,
    {
        LayeredServerBuilder {
            service: layer.layer(self.service),
            router: self.router,
            _api: PhantomData,
            _mounted: PhantomData,
            _provided: PhantomData,
        }
    }
}

impl<A: ApiSpec> Default for ServerBuilder<A, MNil, ENil> {
    fn default() -> Self {
        Self::new()
    }
}
