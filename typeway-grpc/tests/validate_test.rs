use typeway_core::*;
use typeway_grpc::mapping::ToProtoType;
use typeway_grpc::{validate_proto, ApiToProto};
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

#[test]
fn valid_proto_has_no_errors() {
    type API = (
        GetEndpoint<UsersPath, Vec<User>>,
        GetEndpoint<UserByIdPath, User>,
    );
    let proto = API::to_proto("UserService", "users.v1");
    let errors = validate_proto(&proto);
    assert!(
        errors.is_empty(),
        "Expected no errors for valid proto, got: {:?}",
        errors,
    );
}

#[test]
fn duplicate_tags_detected() {
    let proto = r#"syntax = "proto3";

package test.v1;

service TestService {
  rpc GetFoo(FooRequest) returns (FooResponse);
}

message FooRequest {
  string name = 1;
  uint32 id = 1;
}

message FooResponse {
  string value = 1;
}
"#;
    let errors = validate_proto(proto);
    assert!(
        errors.iter().any(|e| e.error.contains("duplicate tag")),
        "Expected duplicate tag error, got: {:?}",
        errors,
    );
}

#[test]
fn zero_tag_detected() {
    let proto = r#"syntax = "proto3";

package test.v1;

message Broken {
  string name = 0;
}
"#;
    let errors = validate_proto(proto);
    assert!(
        errors.iter().any(|e| e.error.contains("tag must be >= 1")),
        "Expected zero tag error, got: {:?}",
        errors,
    );
}

#[test]
fn reserved_range_detected() {
    let proto = r#"syntax = "proto3";

package test.v1;

message Broken {
  string name = 19000;
}
"#;
    let errors = validate_proto(proto);
    assert!(
        errors
            .iter()
            .any(|e| e.error.contains("reserved range 19000-19999")),
        "Expected reserved range error, got: {:?}",
        errors,
    );
}

#[test]
fn reserved_word_field_name_detected() {
    let proto = r#"syntax = "proto3";

package test.v1;

message Broken {
  string message = 1;
}
"#;
    let errors = validate_proto(proto);
    assert!(
        errors
            .iter()
            .any(|e| e.error.contains("proto reserved word")),
        "Expected reserved word error, got: {:?}",
        errors,
    );
}

#[test]
fn invalid_type_detected() {
    let proto = r#"syntax = "proto3";

package test.v1;

message Broken {
  badtype name = 1;
}
"#;
    let errors = validate_proto(proto);
    assert!(
        errors
            .iter()
            .any(|e| e.error.contains("not a valid proto type")),
        "Expected invalid type error, got: {:?}",
        errors,
    );
}

#[test]
fn missing_rpc_type_detected() {
    let proto = r#"syntax = "proto3";

package test.v1;

service TestService {
  rpc GetFoo(MissingRequest) returns (MissingResponse);
}
"#;
    let errors = validate_proto(proto);
    assert!(
        errors.iter().any(|e| e.error.contains("not defined")),
        "Expected missing type error, got: {:?}",
        errors,
    );
    // Should detect both missing input and output types.
    let input_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.error.contains("input type"))
        .collect();
    let output_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.error.contains("output type"))
        .collect();
    assert!(!input_errors.is_empty(), "Expected missing input type error");
    assert!(
        !output_errors.is_empty(),
        "Expected missing output type error"
    );
}

#[test]
fn api_to_proto_validated_returns_errors() {
    // A well-formed API should have no validation errors.
    type API = (
        GetEndpoint<UsersPath, Vec<User>>,
        GetEndpoint<UserByIdPath, User>,
    );
    let (proto, errors) = API::to_proto_validated("UserService", "users.v1");
    assert!(
        !proto.is_empty(),
        "Expected non-empty proto output"
    );
    assert!(
        errors.is_empty(),
        "Expected no errors for valid API, got: {:?}",
        errors,
    );
}
