//! Compile-time readiness check for gRPC.
//!
//! The [`GrpcReady`] trait ensures that all request and response types in an
//! API implement [`ToProtoType`], guaranteeing complete `.proto` generation.
//! This is the gRPC equivalent of
//! [`Serves<A>`](typeway_server::serves::Serves) for the REST server.
//!
//! [`GrpcReady`] is checked as a bound on
//! [`Server::with_grpc`](typeway_server::server::Server::with_grpc), so
//! calling `.with_grpc()` on a server whose API has types missing
//! `ToProtoType` produces a compile-time error.

use typeway_core::effects::{Effect, Requires};
use typeway_core::endpoint::{Endpoint, NoBody};
use typeway_core::method::*;
use typeway_core::path::{ExtractPath, PathSpec};
use typeway_core::versioning::{Deprecated, VersionedApi};
use typeway_core::ApiSpec;

use crate::mapping::ToProtoType;
use crate::streaming::{BidirectionalStream, ClientStream, ServerStream};

/// Compile-time assertion that all request and response types in an API
/// implement [`ToProtoType`], ensuring complete `.proto` generation.
///
/// This is the gRPC equivalent of `Serves<A>` for the REST server --
/// it guarantees the API type is fully compatible with gRPC before
/// the server starts.
///
/// # When does this matter?
///
/// Without `GrpcReady`, calling `.with_grpc()` on a server whose API
/// contains response types that don't implement `ToProtoType` would
/// produce incomplete `.proto` output at runtime. With `GrpcReady`,
/// the compiler catches this at build time.
///
/// # Implementation
///
/// You don't normally implement this trait yourself. It is implemented
/// for all endpoint types whose `Req` and `Res` types implement
/// `ToProtoType`, and for tuples/wrappers composed of such endpoints.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not gRPC-ready: all request and response types must implement `ToProtoType`",
    label = "missing `ToProtoType` implementation for one or more types in this API",
    note = "implement `ToProtoType` for all request/response types used in endpoints, or derive it"
)]
pub trait GrpcReady {}

// ---------------------------------------------------------------------------
// Bodyless endpoints: GET, DELETE, HEAD, OPTIONS — only Res needs ToProtoType
// ---------------------------------------------------------------------------

impl<P: PathSpec + ExtractPath, Res: ToProtoType, Q, Err> GrpcReady
    for Endpoint<Get, P, NoBody, Res, Q, Err>
{
}

impl<P: PathSpec + ExtractPath, Res: ToProtoType, Q, Err> GrpcReady
    for Endpoint<Delete, P, NoBody, Res, Q, Err>
{
}

impl<P: PathSpec + ExtractPath, Res: ToProtoType, Q, Err> GrpcReady
    for Endpoint<Head, P, NoBody, Res, Q, Err>
{
}

impl<P: PathSpec + ExtractPath, Res: ToProtoType, Q, Err> GrpcReady
    for Endpoint<Options, P, NoBody, Res, Q, Err>
{
}

// ---------------------------------------------------------------------------
// Body endpoints: POST, PUT, PATCH — both Req and Res need ToProtoType
// ---------------------------------------------------------------------------

impl<P: PathSpec + ExtractPath, Req: ToProtoType, Res: ToProtoType, Q, Err> GrpcReady
    for Endpoint<Post, P, Req, Res, Q, Err>
{
}

impl<P: PathSpec + ExtractPath, Req: ToProtoType, Res: ToProtoType, Q, Err> GrpcReady
    for Endpoint<Put, P, Req, Res, Q, Err>
{
}

impl<P: PathSpec + ExtractPath, Req: ToProtoType, Res: ToProtoType, Q, Err> GrpcReady
    for Endpoint<Patch, P, Req, Res, Q, Err>
{
}

// ---------------------------------------------------------------------------
// Wrapper types — delegate to the inner endpoint
// ---------------------------------------------------------------------------

impl<E: Effect, Inner: GrpcReady> GrpcReady for Requires<E, Inner> {}

impl<Inner: GrpcReady> GrpcReady for Deprecated<Inner> {}

impl<E: GrpcReady> GrpcReady for ServerStream<E> {}

impl<E: GrpcReady> GrpcReady for ClientStream<E> {}

impl<E: GrpcReady> GrpcReady for BidirectionalStream<E> {}

impl<B, C, R: ApiSpec + GrpcReady> GrpcReady for VersionedApi<B, C, R> {}

// ---------------------------------------------------------------------------
// Tuple impls (arity 1-22)
// ---------------------------------------------------------------------------

macro_rules! impl_grpc_ready_for_tuple {
    ($($T:ident),+) => {
        impl<$($T: GrpcReady,)+> GrpcReady for ($($T,)+) {}
    };
}

impl_grpc_ready_for_tuple!(A);
impl_grpc_ready_for_tuple!(A, B);
impl_grpc_ready_for_tuple!(A, B, C);
impl_grpc_ready_for_tuple!(A, B, C, D);
impl_grpc_ready_for_tuple!(A, B, C, D, E);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U);
impl_grpc_ready_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V);

#[cfg(test)]
mod tests {
    use super::*;
    use typeway_core::endpoint::*;
    use typeway_macros::typeway_path;

    typeway_path!(type UsersPath = "users");
    typeway_path!(type UserByIdPath = "users" / u32);

    // A type that implements ToProtoType.
    struct TestMsg;
    impl ToProtoType for TestMsg {
        fn proto_type_name() -> &'static str {
            "TestMsg"
        }
        fn is_message() -> bool {
            true
        }
        fn message_definition() -> Option<String> {
            Some("message TestMsg {\n  string value = 1;\n}".to_string())
        }
    }

    struct TestReq;
    impl ToProtoType for TestReq {
        fn proto_type_name() -> &'static str {
            "TestReq"
        }
        fn is_message() -> bool {
            true
        }
        fn message_definition() -> Option<String> {
            Some("message TestReq {\n  string data = 1;\n}".to_string())
        }
    }

    fn assert_grpc_ready<T: GrpcReady>() {}

    #[test]
    fn get_endpoint_with_proto_type_is_grpc_ready() {
        assert_grpc_ready::<GetEndpoint<UsersPath, TestMsg>>();
    }

    #[test]
    fn post_endpoint_with_proto_types_is_grpc_ready() {
        assert_grpc_ready::<PostEndpoint<UsersPath, TestReq, TestMsg>>();
    }

    #[test]
    fn delete_endpoint_with_unit_response_is_grpc_ready() {
        assert_grpc_ready::<DeleteEndpoint<UserByIdPath, ()>>();
    }

    #[test]
    fn tuple_of_ready_endpoints_is_grpc_ready() {
        type API = (
            GetEndpoint<UsersPath, Vec<TestMsg>>,
            GetEndpoint<UserByIdPath, TestMsg>,
            PostEndpoint<UsersPath, TestReq, TestMsg>,
            DeleteEndpoint<UserByIdPath, ()>,
        );
        assert_grpc_ready::<API>();
    }

    #[test]
    fn server_stream_is_grpc_ready() {
        assert_grpc_ready::<ServerStream<GetEndpoint<UsersPath, Vec<TestMsg>>>>();
    }

    #[test]
    fn client_stream_is_grpc_ready() {
        assert_grpc_ready::<ClientStream<PostEndpoint<UsersPath, TestReq, TestMsg>>>();
    }

    #[test]
    fn bidirectional_stream_is_grpc_ready() {
        assert_grpc_ready::<BidirectionalStream<PostEndpoint<UsersPath, TestReq, TestMsg>>>();
    }

    #[test]
    fn deprecated_is_grpc_ready() {
        assert_grpc_ready::<Deprecated<GetEndpoint<UsersPath, TestMsg>>>();
    }

    #[test]
    fn string_response_is_grpc_ready() {
        assert_grpc_ready::<GetEndpoint<UsersPath, String>>();
    }

    #[test]
    fn primitive_responses_are_grpc_ready() {
        assert_grpc_ready::<GetEndpoint<UsersPath, u32>>();
        assert_grpc_ready::<GetEndpoint<UsersPath, i64>>();
        assert_grpc_ready::<GetEndpoint<UsersPath, bool>>();
        assert_grpc_ready::<GetEndpoint<UsersPath, f64>>();
    }

    #[test]
    fn option_response_is_grpc_ready() {
        assert_grpc_ready::<GetEndpoint<UsersPath, Option<TestMsg>>>();
    }

    #[test]
    fn status_code_response_is_grpc_ready() {
        assert_grpc_ready::<GetEndpoint<UsersPath, http::StatusCode>>();
    }
}
