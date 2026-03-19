//! The [`ApiSpec`] trait — marks a type as a valid API specification.
//!
//! An API is a tuple of [`Endpoint`](crate::endpoint::Endpoint) types.
//! `ApiSpec` is implemented for individual endpoints and for tuples of
//! `ApiSpec` implementors up to arity 16.

use crate::endpoint::Endpoint;
use crate::method::HttpMethod;
use crate::path::PathSpec;

/// Marker trait for valid API specifications.
///
/// Implemented for:
/// - Individual [`Endpoint`] types
/// - Tuples of `ApiSpec` types (up to arity 16)
///
/// # Example
///
/// ```ignore
/// type MyAPI = (
///     GetEndpoint<path!("users"), Json<Vec<User>>>,
///     PostEndpoint<path!("users"), Json<CreateUser>, Json<User>>,
/// );
/// // MyAPI: ApiSpec ✓
/// ```
pub trait ApiSpec {}

// Every Endpoint is an ApiSpec.
impl<M: HttpMethod, P: PathSpec, Req, Res, Q, Err> ApiSpec for Endpoint<M, P, Req, Res, Q, Err> {}

// Tuples of ApiSpec implementors are ApiSpec.
macro_rules! impl_api_spec_for_tuple {
    ($($T:ident),+) => {
        impl<$($T: ApiSpec),+> ApiSpec for ($($T,)+) {}
    };
}

impl_api_spec_for_tuple!(A);
impl_api_spec_for_tuple!(A, B);
impl_api_spec_for_tuple!(A, B, C);
impl_api_spec_for_tuple!(A, B, C, D);
impl_api_spec_for_tuple!(A, B, C, D, E);
impl_api_spec_for_tuple!(A, B, C, D, E, F);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S);
impl_api_spec_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T);

#[cfg(test)]
#[allow(non_camel_case_types, unused_imports)]
mod tests {
    use super::*;
    use crate::endpoint::*;
    use crate::method::*;
    use crate::path::*;

    struct users;
    impl LitSegment for users {
        const VALUE: &'static str = "users";
    }

    struct User;
    struct CreateUser;

    fn assert_api_spec<A: ApiSpec>() {}

    #[test]
    fn single_endpoint_is_api_spec() {
        type E = GetEndpoint<HCons<Lit<users>, HNil>, User>;
        assert_api_spec::<E>();
    }

    #[test]
    fn tuple_of_endpoints_is_api_spec() {
        type P = HCons<Lit<users>, HNil>;
        type API = (
            GetEndpoint<P, Vec<User>>,
            PostEndpoint<P, CreateUser, User>,
            DeleteEndpoint<HCons<Lit<users>, HCons<Capture<u32>, HNil>>, ()>,
        );
        assert_api_spec::<API>();
    }

    #[test]
    fn nested_tuple_is_api_spec() {
        type P = HCons<Lit<users>, HNil>;
        type Sub1 = (GetEndpoint<P, Vec<User>>,);
        type Sub2 = (PostEndpoint<P, CreateUser, User>,);
        type API = (Sub1, Sub2);
        assert_api_spec::<API>();
    }
}
