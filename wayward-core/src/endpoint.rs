//! The [`Endpoint`] type — a type-level HTTP endpoint descriptor.
//!
//! An endpoint describes a single HTTP route: its method, path, request body
//! type, and response type. It is purely a compile-time construct with no
//! runtime representation.
//!
//! # Example
//!
//! ```ignore
//! type GetUser = Endpoint<Get, HCons<Lit<users>, HCons<Capture<u32>, HNil>>, NoBody, Json<User>>;
//! ```

use std::marker::PhantomData;

use crate::method::*;
use crate::path::PathSpec;

/// A type-level HTTP endpoint descriptor.
///
/// - `M`: HTTP method type (e.g., [`Get`], [`Post`])
/// - `P`: Path HList type (e.g., `HCons<Lit<users>, HCons<Capture<u32>, HNil>>`)
/// - `Req`: Request body type ([`NoBody`] for bodyless methods)
/// - `Res`: Declared response type (for OpenAPI/client generation)
/// - `Q`: Query parameter type (default `()` for no query params). When set,
///   the query params appear in the OpenAPI spec and can be extracted via `Query<Q>`.
pub struct Endpoint<M: HttpMethod, P: PathSpec, Req, Res, Q = ()> {
    _marker: PhantomData<(M, P, Req, Res, Q)>,
}

/// Marker type indicating no request body.
pub struct NoBody;

/// `GET` endpoint with no request body.
pub type GetEndpoint<P, Res, Q = ()> = Endpoint<Get, P, NoBody, Res, Q>;

/// `POST` endpoint with a request body.
pub type PostEndpoint<P, Req, Res, Q = ()> = Endpoint<Post, P, Req, Res, Q>;

/// `PUT` endpoint with a request body.
pub type PutEndpoint<P, Req, Res, Q = ()> = Endpoint<Put, P, Req, Res, Q>;

/// `DELETE` endpoint with no request body.
pub type DeleteEndpoint<P, Res, Q = ()> = Endpoint<Delete, P, NoBody, Res, Q>;

/// `PATCH` endpoint with a request body.
pub type PatchEndpoint<P, Req, Res, Q = ()> = Endpoint<Patch, P, Req, Res, Q>;

#[cfg(test)]
#[allow(non_camel_case_types)]
mod tests {
    use super::*;
    use crate::path::*;

    struct users;
    impl LitSegment for users {
        const VALUE: &'static str = "users";
    }

    #[derive(Debug)]
    struct User;

    #[derive(Debug)]
    struct CreateUser;

    // Verify distinct endpoint types compile and are distinguishable.
    fn assert_distinct<A, B>() {}

    #[test]
    fn endpoint_types_are_distinct() {
        type P = HCons<Lit<users>, HNil>;
        type GetUsers = GetEndpoint<P, Vec<User>>;
        type PostUsers = PostEndpoint<P, CreateUser, User>;
        type DeleteUsers = DeleteEndpoint<P, ()>;

        assert_distinct::<GetUsers, PostUsers>();
        assert_distinct::<GetUsers, DeleteUsers>();
        assert_distinct::<PostUsers, DeleteUsers>();
    }

    #[test]
    fn endpoint_with_captures() {
        type P = HCons<Lit<users>, HCons<Capture<u32>, HNil>>;
        type GetUser = GetEndpoint<P, User>;
        type DeleteUser = DeleteEndpoint<P, ()>;

        assert_distinct::<GetUser, DeleteUser>();
    }
}
