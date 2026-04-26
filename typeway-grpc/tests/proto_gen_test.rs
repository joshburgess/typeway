use typeway_core::*;
use typeway_grpc::*;
use typeway_macros::typeway_path;

typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);

// --- Test domain types with ToProtoType impls ---

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

type TestAPI = (
    GetEndpoint<UsersPath, Vec<User>>,
    GetEndpoint<UserByIdPath, User>,
    PostEndpoint<UsersPath, CreateUser, User>,
    DeleteEndpoint<UserByIdPath, ()>,
);

#[test]
fn generates_proto_file() {
    let proto = TestAPI::to_proto("UserService", "users.v1");
    assert!(proto.contains("syntax = \"proto3\""));
    assert!(proto.contains("package users.v1"));
    assert!(proto.contains("service UserService"));
    assert!(proto.contains("rpc ListUser"));
    assert!(proto.contains("rpc GetUser"));
    assert!(proto.contains("rpc CreateUser"));
    assert!(proto.contains("rpc DeleteUser"));
    // Print for manual inspection.
    println!("{}", proto);
}

#[test]
fn get_with_captures_has_request_message() {
    let proto = TestAPI::to_proto("UserService", "users.v1");
    assert!(
        proto.contains("GetUserRequest"),
        "Expected GetUserRequest in proto:\n{}",
        proto,
    );
}

#[test]
fn post_includes_body_reference() {
    let proto = TestAPI::to_proto("UserService", "users.v1");
    assert!(
        proto.contains("CreateUser"),
        "Expected CreateUser in proto:\n{}",
        proto,
    );
}

#[test]
fn delete_uses_request_with_captures() {
    let proto = TestAPI::to_proto("UserService", "users.v1");
    // DELETE /users/{} should have a request with the capture.
    assert!(
        proto.contains("DeleteUserRequest"),
        "Expected DeleteUserRequest in proto:\n{}",
        proto,
    );
}

#[test]
fn delete_returns_empty() {
    let proto = TestAPI::to_proto("UserService", "users.v1");
    assert!(
        proto.contains("google.protobuf.Empty"),
        "Expected google.protobuf.Empty for DELETE response:\n{}",
        proto,
    );
}

#[test]
fn proto_is_valid_syntax() {
    let proto = TestAPI::to_proto("UserService", "users.v1");
    // Basic syntax checks.
    assert!(proto.starts_with("syntax = \"proto3\""));
    assert!(proto.contains("service UserService {"));
    // Every opened brace should be closed.
    let opens = proto.matches('{').count();
    let closes = proto.matches('}').count();
    assert_eq!(opens, closes, "mismatched braces in:\n{}", proto,);
}

#[test]
fn message_definitions_are_present() {
    let proto = TestAPI::to_proto("UserService", "users.v1");
    assert!(
        proto.contains("message User {"),
        "Expected User message definition:\n{}",
        proto,
    );
    assert!(
        proto.contains("message CreateUserRequest {"),
        "Expected CreateUserRequest message definition:\n{}",
        proto,
    );
}

#[test]
fn list_endpoint_response_is_message_type() {
    let proto = TestAPI::to_proto("UserService", "users.v1");
    // ListUser should return User (the message type, since Vec<User> is repeated).
    assert!(
        proto.contains("rpc ListUser(google.protobuf.Empty) returns (User)"),
        "Expected ListUser to return User:\n{}",
        proto,
    );
}

#[test]
fn single_endpoint_works() {
    type SingleAPI = (GetEndpoint<UsersPath, String>,);
    let proto = SingleAPI::to_proto("SimpleService", "simple.v1");
    assert!(proto.contains("service SimpleService"));
    assert!(proto.contains("rpc ListUser"));
    assert!(proto.contains("ListUserResponse"));
    println!("{}", proto);
}

// --- Request message flattening ---

struct FlatCreateUser;

impl ToProtoType for FlatCreateUser {
    fn proto_type_name() -> &'static str {
        "FlatCreateUser"
    }
    fn is_message() -> bool {
        true
    }
    fn message_definition() -> Option<String> {
        Some("message FlatCreateUser {\n  string name = 1;\n  string email = 2;\n}".to_string())
    }
    fn proto_fields() -> Vec<ProtoField> {
        vec![
            ProtoField {
                name: "name".to_string(),
                proto_type: "string".to_string(),
                tag: 1,
                repeated: false,
                optional: false,
                is_map: false,
                map_key_type: None,
                map_value_type: None,
                doc: None,
            },
            ProtoField {
                name: "email".to_string(),
                proto_type: "string".to_string(),
                tag: 2,
                repeated: false,
                optional: false,
                is_map: false,
                map_key_type: None,
                map_value_type: None,
                doc: None,
            },
        ]
    }
}

#[test]
fn post_with_proto_fields_flattens_request() {
    type FlatAPI = (PostEndpoint<UsersPath, FlatCreateUser, User>,);
    let proto = FlatAPI::to_proto("FlatService", "flat.v1");
    // The request message should contain the flattened fields directly,
    // not a `body` field referencing FlatCreateUser.
    assert!(
        proto.contains("string name = 1"),
        "Expected flattened 'name' field in request:\n{}",
        proto,
    );
    assert!(
        proto.contains("string email = 2"),
        "Expected flattened 'email' field in request:\n{}",
        proto,
    );
    assert!(
        !proto.contains("FlatCreateUser body"),
        "Expected no wrapped 'body' field in request:\n{}",
        proto,
    );
    println!("{}", proto);
}

#[test]
fn post_without_proto_fields_wraps_body() {
    // CreateUser doesn't implement proto_fields(), so it should stay wrapped.
    type WrappedAPI = (PostEndpoint<UsersPath, CreateUser, User>,);
    let proto = WrappedAPI::to_proto("WrappedService", "wrapped.v1");
    assert!(
        proto.contains("CreateUser body"),
        "Expected wrapped 'body' field in request:\n{}",
        proto,
    );
    println!("{}", proto);
}

#[test]
fn post_with_captures_and_flattening() {
    type CaptureAPI = (PostEndpoint<UserByIdPath, FlatCreateUser, User>,);
    let proto = CaptureAPI::to_proto("CaptureService", "capture.v1");
    // Should have the capture field (param1 = 1) plus flattened fields (name = 2, email = 3).
    assert!(
        proto.contains("param1"),
        "Expected capture field in request:\n{}",
        proto,
    );
    assert!(
        proto.contains("string name = 2"),
        "Expected flattened 'name' field with offset tag:\n{}",
        proto,
    );
    assert!(
        proto.contains("string email = 3"),
        "Expected flattened 'email' field with offset tag:\n{}",
        proto,
    );
    println!("{}", proto);
}
