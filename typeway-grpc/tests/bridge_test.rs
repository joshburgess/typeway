//! Integration tests for Phase B: service descriptors, status mapping, and bridge.

use typeway_grpc::service::ApiToServiceDescriptor;
use typeway_grpc::status::{http_to_grpc_code, GrpcCode};

// ---------------------------------------------------------------------------
// We use a real API type to test ApiToServiceDescriptor.
// This mirrors the pattern from proto_gen_test.rs.
// ---------------------------------------------------------------------------

use typeway_core::endpoint::{GetEndpoint, PostEndpoint, DeleteEndpoint};
use typeway_core::path::{Capture, HCons, HNil, Lit, LitSegment};

use typeway_grpc::mapping::ToProtoType;

#[allow(non_camel_case_types)]
struct users;
impl LitSegment for users {
    const VALUE: &'static str = "users";
}

#[derive(Debug)]
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

#[derive(Debug)]
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

type UsersPath = HCons<Lit<users>, HNil>;
type UserByIdPath = HCons<Lit<users>, HCons<Capture<u32>, HNil>>;

type TestAPI = (
    GetEndpoint<UsersPath, Vec<User>>,
    GetEndpoint<UserByIdPath, User>,
    PostEndpoint<UsersPath, CreateUser, User>,
    DeleteEndpoint<UserByIdPath, ()>,
);

// ---------------------------------------------------------------------------
// Service descriptor tests
// ---------------------------------------------------------------------------

#[test]
fn service_descriptor_from_api_type() {
    let desc = TestAPI::service_descriptor("UserService", "users.v1");
    assert_eq!(desc.name, "UserService");
    assert_eq!(desc.package, "users.v1");
    assert_eq!(desc.methods.len(), 4);
}

#[test]
fn method_descriptors_have_correct_full_paths() {
    let desc = TestAPI::service_descriptor("UserService", "users.v1");
    let paths: Vec<&str> = desc.methods.iter().map(|m| m.full_path.as_str()).collect();

    assert!(paths.contains(&"/users.v1.UserService/ListUser"));
    assert!(paths.contains(&"/users.v1.UserService/GetUser"));
    assert!(paths.contains(&"/users.v1.UserService/CreateUser"));
    assert!(paths.contains(&"/users.v1.UserService/DeleteUser"));
}

#[test]
fn method_descriptors_have_correct_http_methods() {
    let desc = TestAPI::service_descriptor("UserService", "users.v1");

    let list = desc.find_method("/users.v1.UserService/ListUser").unwrap();
    assert_eq!(list.http_method, http::Method::GET);

    let get = desc.find_method("/users.v1.UserService/GetUser").unwrap();
    assert_eq!(get.http_method, http::Method::GET);

    let create = desc.find_method("/users.v1.UserService/CreateUser").unwrap();
    assert_eq!(create.http_method, http::Method::POST);

    let delete = desc.find_method("/users.v1.UserService/DeleteUser").unwrap();
    assert_eq!(delete.http_method, http::Method::DELETE);
}

#[test]
fn method_descriptors_have_rest_paths() {
    let desc = TestAPI::service_descriptor("UserService", "users.v1");

    let list = desc.find_method("/users.v1.UserService/ListUser").unwrap();
    assert_eq!(list.rest_path, "/users");

    let get = desc.find_method("/users.v1.UserService/GetUser").unwrap();
    assert_eq!(get.rest_path, "/users/{}");
}

#[test]
fn find_method_returns_none_for_unknown() {
    let desc = TestAPI::service_descriptor("UserService", "users.v1");
    assert!(desc.find_method("/users.v1.UserService/UpdateUser").is_none());
}

// ---------------------------------------------------------------------------
// Status mapping tests
// ---------------------------------------------------------------------------

#[test]
fn http_to_grpc_common_mappings() {
    assert_eq!(http_to_grpc_code(http::StatusCode::OK), GrpcCode::Ok);
    assert_eq!(http_to_grpc_code(http::StatusCode::CREATED), GrpcCode::Ok);
    assert_eq!(
        http_to_grpc_code(http::StatusCode::BAD_REQUEST),
        GrpcCode::InvalidArgument
    );
    assert_eq!(
        http_to_grpc_code(http::StatusCode::UNAUTHORIZED),
        GrpcCode::Unauthenticated
    );
    assert_eq!(
        http_to_grpc_code(http::StatusCode::FORBIDDEN),
        GrpcCode::PermissionDenied
    );
    assert_eq!(
        http_to_grpc_code(http::StatusCode::NOT_FOUND),
        GrpcCode::NotFound
    );
    assert_eq!(
        http_to_grpc_code(http::StatusCode::CONFLICT),
        GrpcCode::AlreadyExists
    );
    assert_eq!(
        http_to_grpc_code(http::StatusCode::TOO_MANY_REQUESTS),
        GrpcCode::ResourceExhausted
    );
    assert_eq!(
        http_to_grpc_code(http::StatusCode::INTERNAL_SERVER_ERROR),
        GrpcCode::Internal
    );
    assert_eq!(
        http_to_grpc_code(http::StatusCode::NOT_IMPLEMENTED),
        GrpcCode::Unimplemented
    );
    assert_eq!(
        http_to_grpc_code(http::StatusCode::SERVICE_UNAVAILABLE),
        GrpcCode::Unavailable
    );
    assert_eq!(
        http_to_grpc_code(http::StatusCode::GATEWAY_TIMEOUT),
        GrpcCode::DeadlineExceeded
    );
}

#[test]
fn unmapped_http_codes_return_unknown() {
    assert_eq!(
        http_to_grpc_code(http::StatusCode::IM_A_TEAPOT),
        GrpcCode::Unknown
    );
    assert_eq!(
        http_to_grpc_code(http::StatusCode::GONE),
        GrpcCode::Unknown
    );
    assert_eq!(
        http_to_grpc_code(http::StatusCode::PAYMENT_REQUIRED),
        GrpcCode::Unknown
    );
}

// ---------------------------------------------------------------------------
// Bridge compile test
// ---------------------------------------------------------------------------

#[test]
fn grpc_bridge_can_be_constructed() {
    use typeway_grpc::bridge::GrpcBridge;

    #[derive(Clone)]
    struct DummyService;

    let desc = TestAPI::service_descriptor("UserService", "users.v1");
    let bridge = GrpcBridge::new(DummyService, desc);

    // Verify the descriptor is accessible.
    assert_eq!(bridge.descriptor().name, "UserService");
    assert_eq!(bridge.descriptor().methods.len(), 4);
}

#[test]
fn grpc_bridge_from_api_constructor() {
    use typeway_grpc::bridge::GrpcBridge;

    #[derive(Clone)]
    struct DummyService;

    let bridge = GrpcBridge::from_api::<TestAPI>(DummyService, "UserService", "users.v1");

    assert_eq!(bridge.descriptor().name, "UserService");
    assert_eq!(bridge.descriptor().package, "users.v1");
    assert_eq!(bridge.descriptor().methods.len(), 4);
}

#[test]
fn grpc_bridge_is_cloneable() {
    use typeway_grpc::bridge::GrpcBridge;

    #[derive(Clone)]
    struct DummyService;

    let bridge = GrpcBridge::from_api::<TestAPI>(DummyService, "UserService", "users.v1");
    let bridge2 = bridge.clone();
    assert_eq!(bridge2.descriptor().methods.len(), 4);
}

#[test]
fn different_package_and_service_names() {
    let desc = TestAPI::service_descriptor("AccountService", "accounts.v2");
    assert_eq!(desc.name, "AccountService");
    assert_eq!(desc.package, "accounts.v2");

    let method = desc.find_method("/accounts.v2.AccountService/ListUser");
    assert!(method.is_some());
    assert_eq!(method.unwrap().name, "ListUser");
}
