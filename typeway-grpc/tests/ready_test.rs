//! Tests for the `GrpcReady` compile-time readiness check.
//!
//! These tests verify that endpoint types with proper `ToProtoType`
//! implementations satisfy `GrpcReady`, and that wrapper types
//! (streaming, deprecated, requires) correctly delegate readiness.
#![allow(dead_code)]

use typeway_core::effects::{CorsRequired, Requires};
use typeway_core::endpoint::*;
use typeway_core::versioning::Deprecated;
use typeway_grpc::mapping::ToProtoType;
use typeway_grpc::streaming::{BidirectionalStream, ClientStream, ServerStream};
use typeway_grpc::GrpcReady;
use typeway_macros::typeway_path;

typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);
typeway_path!(type PostsPath = "users" / u32 / "posts");

// --- Test domain types ---

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

struct CreateUser;

impl ToProtoType for CreateUser {
    fn proto_type_name() -> &'static str {
        "CreateUser"
    }
    fn is_message() -> bool {
        true
    }
    fn message_definition() -> Option<String> {
        Some("message CreateUser {\n  string name = 1;\n}".to_string())
    }
}

fn assert_grpc_ready<T: GrpcReady>() {}

// --- Endpoint readiness ---

#[test]
fn get_endpoint_is_grpc_ready() {
    assert_grpc_ready::<GetEndpoint<UsersPath, Vec<User>>>();
}

#[test]
fn get_endpoint_with_capture_is_grpc_ready() {
    assert_grpc_ready::<GetEndpoint<UserByIdPath, User>>();
}

#[test]
fn post_endpoint_is_grpc_ready() {
    assert_grpc_ready::<PostEndpoint<UsersPath, CreateUser, User>>();
}

#[test]
fn delete_endpoint_is_grpc_ready() {
    assert_grpc_ready::<DeleteEndpoint<UserByIdPath, ()>>();
}

#[test]
fn put_endpoint_is_grpc_ready() {
    assert_grpc_ready::<PutEndpoint<UserByIdPath, CreateUser, User>>();
}

#[test]
fn patch_endpoint_is_grpc_ready() {
    assert_grpc_ready::<PatchEndpoint<UserByIdPath, CreateUser, User>>();
}

// --- Tuple readiness ---

#[test]
fn tuple_of_ready_endpoints_is_grpc_ready() {
    type API = (
        GetEndpoint<UsersPath, Vec<User>>,
        GetEndpoint<UserByIdPath, User>,
        PostEndpoint<UsersPath, CreateUser, User>,
        DeleteEndpoint<UserByIdPath, ()>,
    );
    assert_grpc_ready::<API>();
}

#[test]
fn single_element_tuple_is_grpc_ready() {
    assert_grpc_ready::<(GetEndpoint<UsersPath, String>,)>();
}

// --- Wrapper readiness ---

#[test]
fn server_stream_is_grpc_ready() {
    assert_grpc_ready::<ServerStream<GetEndpoint<UsersPath, Vec<User>>>>();
}

#[test]
fn client_stream_is_grpc_ready() {
    assert_grpc_ready::<ClientStream<PostEndpoint<UsersPath, CreateUser, User>>>();
}

#[test]
fn bidirectional_stream_is_grpc_ready() {
    assert_grpc_ready::<BidirectionalStream<PostEndpoint<UsersPath, CreateUser, User>>>();
}

#[test]
fn deprecated_is_grpc_ready() {
    assert_grpc_ready::<Deprecated<GetEndpoint<UsersPath, User>>>();
}

#[test]
fn requires_wrapper_is_grpc_ready() {
    assert_grpc_ready::<Requires<CorsRequired, GetEndpoint<UsersPath, User>>>();
}

#[test]
fn nested_wrappers_are_grpc_ready() {
    // Requires wrapping a Deprecated wrapping a ServerStream
    assert_grpc_ready::<
        Requires<CorsRequired, Deprecated<ServerStream<GetEndpoint<UsersPath, Vec<User>>>>>,
    >();
}

// --- Primitive response types ---

#[test]
fn primitive_response_types_are_grpc_ready() {
    assert_grpc_ready::<GetEndpoint<UsersPath, String>>();
    assert_grpc_ready::<GetEndpoint<UsersPath, u32>>();
    assert_grpc_ready::<GetEndpoint<UsersPath, i64>>();
    assert_grpc_ready::<GetEndpoint<UsersPath, bool>>();
    assert_grpc_ready::<GetEndpoint<UsersPath, f64>>();
    assert_grpc_ready::<GetEndpoint<UsersPath, Vec<u8>>>();
}

#[test]
fn option_response_is_grpc_ready() {
    assert_grpc_ready::<GetEndpoint<UsersPath, Option<User>>>();
}

#[test]
fn status_code_response_is_grpc_ready() {
    assert_grpc_ready::<GetEndpoint<UsersPath, http::StatusCode>>();
}

// --- Mixed API with streaming ---

#[test]
fn mixed_api_with_streaming_is_grpc_ready() {
    type MixedAPI = (
        GetEndpoint<UsersPath, Vec<User>>,
        ServerStream<GetEndpoint<UsersPath, Vec<User>>>,
        PostEndpoint<UsersPath, CreateUser, User>,
        DeleteEndpoint<UserByIdPath, ()>,
    );
    assert_grpc_ready::<MixedAPI>();
}

// --- grpc_client! compile-time check ---

// The grpc_client! macro generates a client struct from the API type.
// Verify the macro expands without error.
#[cfg(feature = "grpc-native")]
#[test]
fn grpc_client_compiles() {
    type TestAPI = (
        GetEndpoint<UsersPath, Vec<User>>,
        GetEndpoint<UserByIdPath, User>,
        PostEndpoint<UsersPath, CreateUser, User>,
    );

    typeway_grpc::grpc_client! {
        struct TestClient;
        api = TestAPI;
        service = "UserService";
        package = "users.v1";
    }

    // Verify the struct was created and has the expected methods.
    let _: fn(&str) -> Result<TestClient, typeway_grpc::GrpcClientError> = TestClient::new;
}

#[cfg(feature = "grpc-native")]
#[test]
fn grpc_client_service_descriptor() {
    type TestAPI = (
        GetEndpoint<UsersPath, Vec<User>>,
        GetEndpoint<UserByIdPath, User>,
    );

    typeway_grpc::grpc_client! {
        struct DescClient;
        api = TestAPI;
        service = "TestService";
        package = "test.v1";
    }

    let client = DescClient::new("http://localhost:50051").unwrap();
    let desc = client.service_descriptor();

    assert_eq!(desc.name, "TestService");
    assert_eq!(desc.package, "test.v1");
    assert_eq!(desc.methods.len(), 2);
    assert_eq!(desc.methods[0].name, "ListUser");
    assert_eq!(desc.methods[1].name, "GetUser");
}

#[cfg(feature = "grpc-native")]
#[test]
fn grpc_client_proto_generation() {
    type TestAPI = (
        GetEndpoint<UsersPath, Vec<User>>,
        PostEndpoint<UsersPath, CreateUser, User>,
    );

    typeway_grpc::grpc_client! {
        struct ProtoClient;
        api = TestAPI;
        service = "UserService";
        package = "users.v1";
    }

    let client = ProtoClient::new("http://localhost:50051").unwrap();
    let proto = client.proto();

    assert!(proto.contains("syntax = \"proto3\""));
    assert!(proto.contains("package users.v1"));
    assert!(proto.contains("service UserService"));
    assert!(proto.contains("rpc ListUser"));
    assert!(proto.contains("rpc CreateUser"));
}
