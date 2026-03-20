//! Tests for the gRPC server reflection service.

use typeway_grpc::reflection::ReflectionService;

fn test_reflection() -> ReflectionService {
    ReflectionService::new(
        concat!(
            "syntax = \"proto3\";\n",
            "package users.v1;\n",
            "service UserService {\n",
            "  rpc ListUser(google.protobuf.Empty) returns (ListUserResponse);\n",
            "  rpc GetUser(GetUserRequest) returns (User);\n",
            "}\n",
        )
        .to_string(),
        vec!["users.v1.UserService".to_string()],
    )
}

#[test]
fn list_services_returns_correct_name() {
    let svc = test_reflection();
    assert_eq!(svc.list_services(), &["users.v1.UserService"]);
}

#[test]
fn file_containing_symbol_returns_proto_content() {
    let svc = test_reflection();
    let proto = svc.file_containing_symbol("users.v1.UserService");
    assert!(proto.is_some());
    let content = proto.unwrap();
    assert!(content.contains("UserService"));
    assert!(content.contains("proto3"));
}

#[test]
fn file_containing_symbol_returns_proto_regardless_of_symbol() {
    let svc = test_reflection();
    // Even for an unknown symbol, the simplified implementation returns the proto.
    let proto = svc.file_containing_symbol("totally.unknown.Symbol");
    assert!(proto.is_some());
}

#[test]
fn handle_request_list_services_returns_json() {
    let svc = test_reflection();
    let response = svc.handle_request("{\"list_services\":\"\"}");
    assert!(response.contains("listServicesResponse"));
    assert!(response.contains("users.v1.UserService"));
}

#[test]
fn handle_request_file_returns_proto_content() {
    let svc = test_reflection();
    let response = svc.handle_request("{\"file_containing_symbol\":\"users.v1.UserService\"}");
    assert!(response.contains("fileDescriptorResponse"));
    assert!(response.contains("UserService"));
}

#[test]
fn proto_content_accessor() {
    let svc = test_reflection();
    let content = svc.proto_content();
    assert!(content.contains("syntax = \"proto3\""));
    assert!(content.contains("package users.v1"));
}

#[test]
fn is_reflection_path_matches_exact() {
    assert!(ReflectionService::is_reflection_path(
        "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo"
    ));
}

#[test]
fn is_reflection_path_matches_prefix() {
    assert!(ReflectionService::is_reflection_path(
        "/grpc.reflection.v1alpha/SomeOther"
    ));
}

#[test]
fn is_reflection_path_rejects_non_reflection() {
    assert!(!ReflectionService::is_reflection_path("/users.v1.UserService/GetUser"));
    assert!(!ReflectionService::is_reflection_path("/grpc.health.v1.Health/Check"));
    assert!(!ReflectionService::is_reflection_path("/"));
}

#[test]
fn multiple_services() {
    let svc = ReflectionService::new(
        "proto content".to_string(),
        vec![
            "pkg.v1.ServiceA".to_string(),
            "pkg.v1.ServiceB".to_string(),
        ],
    );
    assert_eq!(svc.list_services().len(), 2);
    let response = svc.handle_request("{\"list_services\":\"\"}");
    assert!(response.contains("ServiceA"));
    assert!(response.contains("ServiceB"));
}
