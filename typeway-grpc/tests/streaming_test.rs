use typeway_core::*;
use typeway_grpc::mapping::ToProtoType;
use typeway_grpc::streaming::ServerStream;
use typeway_grpc::{ApiToProto, CollectRpcs};
use typeway_macros::typeway_path;

typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);

struct User;

impl ToProtoType for User {
    fn proto_type_name() -> &'static str {
        "User"
    }
    fn is_message() -> bool {
        true
    }
    fn message_definition() -> Option<String> {
        Some("message User {\n  uint32 id = 1;\n  string name = 2;\n}".to_string())
    }
}

/// Verify that `ServerStream<E>` implements `ApiSpec` when `E` does.
fn assert_api_spec<T: ApiSpec>() {}

#[test]
fn server_stream_implements_api_spec() {
    assert_api_spec::<ServerStream<GetEndpoint<UsersPath, Vec<User>>>>();
}

#[test]
fn server_stream_rpc_has_streaming_flag() {
    type E = ServerStream<GetEndpoint<UsersPath, Vec<User>>>;
    let rpcs = E::collect_rpcs();
    assert_eq!(rpcs.len(), 1);
    assert!(
        rpcs[0].server_streaming,
        "Expected server_streaming to be true"
    );
}

#[test]
fn non_streaming_rpc_has_no_streaming_flag() {
    type E = GetEndpoint<UsersPath, Vec<User>>;
    let rpcs = E::collect_rpcs();
    assert_eq!(rpcs.len(), 1);
    assert!(
        !rpcs[0].server_streaming,
        "Expected server_streaming to be false for non-streaming endpoint"
    );
}

#[test]
fn proto_output_contains_stream_keyword() {
    type API = (
        GetEndpoint<UserByIdPath, User>,
        ServerStream<GetEndpoint<UsersPath, Vec<User>>>,
    );
    let proto = API::to_proto("UserService", "users.v1");
    // The streaming endpoint should have "returns (stream User)".
    assert!(
        proto.contains("returns (stream "),
        "Expected 'returns (stream ...)' in proto:\n{}",
        proto,
    );
    // The non-streaming endpoint should NOT have "stream" in its returns.
    assert!(
        proto.contains("rpc GetUser(GetUserRequest) returns (User);"),
        "Expected non-streaming GetUser in proto:\n{}",
        proto,
    );
    println!("{}", proto);
}

#[test]
fn server_stream_in_tuple_api() {
    type API = (
        ServerStream<GetEndpoint<UsersPath, Vec<User>>>,
        GetEndpoint<UserByIdPath, User>,
    );
    assert_api_spec::<API>();
    let rpcs = API::collect_rpcs();
    assert_eq!(rpcs.len(), 2);
    // First should be streaming, second should not.
    assert!(rpcs[0].server_streaming);
    assert!(!rpcs[1].server_streaming);
}
