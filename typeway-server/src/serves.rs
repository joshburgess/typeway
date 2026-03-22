//! The [`Serves`] trait — compile-time check that a handler tuple covers an API.
//!
//! `Serves<A>` is implemented for tuples of [`BoundHandler`]
//! that cover every endpoint in the API type `A`.

use typeway_core::ApiSpec;

use crate::handler_for::BoundHandler;
use crate::router::Router;

/// A tuple of bound handlers that fully covers an API specification.
///
/// If your API has 3 endpoints and you provide only 2 handlers, the compiler
/// will reject it because `Serves<API>` won't be satisfied.
#[diagnostic::on_unimplemented(
    message = "the handler tuple does not match the API specification `{A}`",
    label = "handler tuple does not implement `Serves<{A}>`",
    note = "each endpoint in the API type needs a corresponding `BoundHandler` created via `bind!(handler)`",
    note = "the handler tuple must have exactly one `bind!(handler)` for each endpoint"
)]
pub trait Serves<A: ApiSpec> {
    /// Register all handlers into the router.
    fn register(self, router: &mut Router);
}

// Generate Serves impls for tuples of BoundHandler matching tuples of endpoints.
macro_rules! impl_serves_for_tuple {
    ($(($E:ident, $idx:tt)),+) => {
        impl<$($E: ApiSpec,)+> Serves<($($E,)+)> for ($(BoundHandler<$E>,)+) {
            fn register(self, router: &mut Router) {
                $(self.$idx.register_into(router);)+
            }
        }
    };
}

impl_serves_for_tuple!((E0, 0));
impl_serves_for_tuple!((E0, 0), (E1, 1));
impl_serves_for_tuple!((E0, 0), (E1, 1), (E2, 2));
impl_serves_for_tuple!((E0, 0), (E1, 1), (E2, 2), (E3, 3));
impl_serves_for_tuple!((E0, 0), (E1, 1), (E2, 2), (E3, 3), (E4, 4));
impl_serves_for_tuple!((E0, 0), (E1, 1), (E2, 2), (E3, 3), (E4, 4), (E5, 5));
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10),
    (E11, 11)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10),
    (E11, 11),
    (E12, 12)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10),
    (E11, 11),
    (E12, 12),
    (E13, 13)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10),
    (E11, 11),
    (E12, 12),
    (E13, 13),
    (E14, 14)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10),
    (E11, 11),
    (E12, 12),
    (E13, 13),
    (E14, 14),
    (E15, 15)
);

// Extended to arity 20 for large APIs.
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10),
    (E11, 11),
    (E12, 12),
    (E13, 13),
    (E14, 14),
    (E15, 15),
    (E16, 16)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10),
    (E11, 11),
    (E12, 12),
    (E13, 13),
    (E14, 14),
    (E15, 15),
    (E16, 16),
    (E17, 17)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10),
    (E11, 11),
    (E12, 12),
    (E13, 13),
    (E14, 14),
    (E15, 15),
    (E16, 16),
    (E17, 17),
    (E18, 18)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10),
    (E11, 11),
    (E12, 12),
    (E13, 13),
    (E14, 14),
    (E15, 15),
    (E16, 16),
    (E17, 17),
    (E18, 18),
    (E19, 19)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10),
    (E11, 11),
    (E12, 12),
    (E13, 13),
    (E14, 14),
    (E15, 15),
    (E16, 16),
    (E17, 17),
    (E18, 18),
    (E19, 19),
    (E20, 20)
);
impl_serves_for_tuple!(
    (E0, 0),
    (E1, 1),
    (E2, 2),
    (E3, 3),
    (E4, 4),
    (E5, 5),
    (E6, 6),
    (E7, 7),
    (E8, 8),
    (E9, 9),
    (E10, 10),
    (E11, 11),
    (E12, 12),
    (E13, 13),
    (E14, 14),
    (E15, 15),
    (E16, 16),
    (E17, 17),
    (E18, 18),
    (E19, 19),
    (E20, 20),
    (E21, 21)
);

// VersionedApi<Base, Changes, Resolved> delegates Serves to the resolved API type.
// This allows EffectfulServer::<VersionedApi<V1, Changes, V2Resolved>>::new(handlers)
// where handlers: Serves<V2Resolved>.
impl<B, C, R: ApiSpec, H: Serves<R>> Serves<typeway_core::versioning::VersionedApi<B, C, R>>
    for H
{
    fn register(self, router: &mut Router) {
        <H as Serves<R>>::register(self, router)
    }
}

// ---------------------------------------------------------------------------
// Nested API composition — for APIs with more than 22 endpoints.
//
// Use `SubApi<A, H>` to compose sub-APIs with their handlers:
//
//   type FullAPI = (UsersAPI, OrdersAPI);
//   Server::<FullAPI>::new((
//       SubApi::<UsersAPI, _>::new((bind!(get_users), bind!(create_user))),
//       SubApi::<OrdersAPI, _>::new((bind!(get_orders),)),
//   ))
// ---------------------------------------------------------------------------

/// A wrapper that pairs a sub-API with its handler tuple.
///
/// Allows composing APIs larger than 22 endpoints by nesting sub-APIs.
///
/// # Example
///
/// ```ignore
/// type UsersAPI = (GetEndpoint<UsersPath, Vec<User>>, PostEndpoint<UsersPath, CreateUser, User>);
/// type OrdersAPI = (GetEndpoint<OrdersPath, Vec<Order>>,);
/// type FullAPI = (UsersAPI, OrdersAPI);
///
/// Server::<FullAPI>::new((
///     SubApi::<UsersAPI, _>::new((bind!(list_users), bind!(create_user))),
///     SubApi::<OrdersAPI, _>::new((bind!(list_orders),)),
/// ))
/// ```
/// A wrapper that pairs a sub-API type with its handler tuple for
/// nested API composition.
pub struct SubApi<A: ApiSpec, H> {
    handlers: H,
    _api: std::marker::PhantomData<A>,
}

impl<A: ApiSpec, H> SubApi<A, H> {
    pub fn new(handlers: H) -> Self {
        SubApi {
            handlers,
            _api: std::marker::PhantomData,
        }
    }
}

impl<A: ApiSpec, H: Serves<A>> SubApi<A, H> {
    /// Register all handlers from this sub-API into the router.
    pub fn register_into(self, router: &mut Router) {
        self.handlers.register(router);
    }
}

// SubApi<A, H> implements Serves<A> — but we need to avoid the VersionedApi conflict.
// Instead, we generate Serves impls for tuples of SubApi via a macro:

macro_rules! impl_serves_for_subapi_tuple {
    ($(($A:ident, $H:ident, $idx:tt)),+) => {
        impl<$($A: ApiSpec, $H: Serves<$A>,)+> Serves<($($A,)+)> for ($(SubApi<$A, $H>,)+) {
            fn register(self, router: &mut Router) {
                $(self.$idx.register_into(router);)+
            }
        }
    };
}

impl_serves_for_subapi_tuple!((A0, H0, 0), (A1, H1, 1));
impl_serves_for_subapi_tuple!((A0, H0, 0), (A1, H1, 1), (A2, H2, 2));
impl_serves_for_subapi_tuple!((A0, H0, 0), (A1, H1, 1), (A2, H2, 2), (A3, H3, 3));
impl_serves_for_subapi_tuple!((A0, H0, 0), (A1, H1, 1), (A2, H2, 2), (A3, H3, 3), (A4, H4, 4));
impl_serves_for_subapi_tuple!((A0, H0, 0), (A1, H1, 1), (A2, H2, 2), (A3, H3, 3), (A4, H4, 4), (A5, H5, 5));
impl_serves_for_subapi_tuple!((A0, H0, 0), (A1, H1, 1), (A2, H2, 2), (A3, H3, 3), (A4, H4, 4), (A5, H5, 5), (A6, H6, 6));
impl_serves_for_subapi_tuple!((A0, H0, 0), (A1, H1, 1), (A2, H2, 2), (A3, H3, 3), (A4, H4, 4), (A5, H5, 5), (A6, H6, 6), (A7, H7, 7));
impl_serves_for_subapi_tuple!((A0, H0, 0), (A1, H1, 1), (A2, H2, 2), (A3, H3, 3), (A4, H4, 4), (A5, H5, 5), (A6, H6, 6), (A7, H7, 7), (A8, H8, 8));
impl_serves_for_subapi_tuple!((A0, H0, 0), (A1, H1, 1), (A2, H2, 2), (A3, H3, 3), (A4, H4, 4), (A5, H5, 5), (A6, H6, 6), (A7, H7, 7), (A8, H8, 8), (A9, H9, 9));
