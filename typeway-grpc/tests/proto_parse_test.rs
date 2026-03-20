use typeway_grpc::codegen::generate_typeway_from_proto;
use typeway_grpc::proto_parse::parse_proto;

const USERS_PROTO: &str = include_str!("fixtures/users.proto");

#[test]
fn parses_syntax() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    assert_eq!(proto.syntax, "proto3");
}

#[test]
fn parses_package() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    assert_eq!(proto.package, "users.v1");
}

#[test]
fn parses_service() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    assert_eq!(proto.services.len(), 1);
    assert_eq!(proto.services[0].name, "UserService");
    assert_eq!(proto.services[0].methods.len(), 4);
}

#[test]
fn parses_rpc_methods() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    let methods = &proto.services[0].methods;

    assert_eq!(methods[0].name, "ListUser");
    assert_eq!(methods[0].input_type, "google.protobuf.Empty");
    assert_eq!(methods[0].output_type, "ListUserResponse");

    assert_eq!(methods[1].name, "GetUser");
    assert_eq!(methods[1].input_type, "GetUserRequest");
    assert_eq!(methods[1].output_type, "User");

    assert_eq!(methods[2].name, "CreateUser");
    assert_eq!(methods[2].input_type, "CreateUserRequest");
    assert_eq!(methods[2].output_type, "User");

    assert_eq!(methods[3].name, "DeleteUser");
    assert_eq!(methods[3].input_type, "DeleteUserRequest");
    assert_eq!(methods[3].output_type, "google.protobuf.Empty");
}

#[test]
fn parses_rpc_comments() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    let methods = &proto.services[0].methods;

    assert_eq!(methods[0].comment.as_deref(), Some("// GET /users"));
    assert_eq!(methods[1].comment.as_deref(), Some("// GET /users/{}"));
    assert_eq!(methods[2].comment.as_deref(), Some("// POST /users"));
    assert_eq!(methods[3].comment.as_deref(), Some("// DELETE /users/{}"));
}

#[test]
fn parses_messages() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    assert_eq!(proto.messages.len(), 5);

    let names: Vec<&str> = proto.messages.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"User"));
    assert!(names.contains(&"GetUserRequest"));
    assert!(names.contains(&"CreateUserRequest"));
    assert!(names.contains(&"DeleteUserRequest"));
    assert!(names.contains(&"ListUserResponse"));
}

#[test]
fn parses_field_types() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    let user = proto.messages.iter().find(|m| m.name == "User").unwrap();
    assert_eq!(user.fields.len(), 3);
    assert_eq!(user.fields[0].proto_type, "uint32");
    assert_eq!(user.fields[0].name, "id");
    assert_eq!(user.fields[0].tag, 1);
    assert_eq!(user.fields[1].proto_type, "string");
    assert_eq!(user.fields[1].name, "name");
    assert_eq!(user.fields[2].proto_type, "string");
    assert_eq!(user.fields[2].name, "email");
}

#[test]
fn parses_repeated_fields() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    let list_resp = proto
        .messages
        .iter()
        .find(|m| m.name == "ListUserResponse")
        .unwrap();
    assert_eq!(list_resp.fields.len(), 1);
    assert!(list_resp.fields[0].repeated);
    assert_eq!(list_resp.fields[0].proto_type, "User");
    assert_eq!(list_resp.fields[0].name, "users");
}

#[test]
fn generates_rust_structs() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    let output = generate_typeway_from_proto(&proto);
    assert!(output.contains("pub struct User {"));
    assert!(output.contains("pub id: u32,"));
    assert!(output.contains("pub name: String,"));
    assert!(output.contains("pub email: String,"));
}

#[test]
fn generates_typeway_path() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    let output = generate_typeway_from_proto(&proto);
    assert!(output.contains("typeway_path!"));
}

#[test]
fn generates_api_type() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    let output = generate_typeway_from_proto(&proto);
    assert!(output.contains("type API = ("));
}

#[test]
fn generated_code_mentions_all_endpoints() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    let output = generate_typeway_from_proto(&proto);
    assert!(output.contains("GetEndpoint"), "missing GetEndpoint");
    assert!(output.contains("PostEndpoint"), "missing PostEndpoint");
    assert!(output.contains("DeleteEndpoint"), "missing DeleteEndpoint");
}

#[test]
fn roundtrip_proto_to_rust() {
    let proto = parse_proto(USERS_PROTO).unwrap();
    let output = generate_typeway_from_proto(&proto);

    // All message structs present.
    assert!(output.contains("pub struct User {"));
    assert!(output.contains("pub struct GetUserRequest {"));
    assert!(output.contains("pub struct CreateUserRequest {"));
    assert!(output.contains("pub struct DeleteUserRequest {"));
    assert!(output.contains("pub struct ListUserResponse {"));

    // API type present.
    assert!(output.contains("type API = ("));

    // Path declarations present.
    assert!(output.contains("typeway_path!(type"));

    // Endpoint types present.
    assert!(output.contains("GetEndpoint"));
    assert!(output.contains("PostEndpoint"));
    assert!(output.contains("DeleteEndpoint"));

    // Serde derives on structs.
    assert!(output.contains("#[derive(Debug, Clone, Serialize, Deserialize)]"));

    // Use statements.
    assert!(output.contains("use typeway::prelude::*;"));
    assert!(output.contains("use serde::{Serialize, Deserialize};"));
}
