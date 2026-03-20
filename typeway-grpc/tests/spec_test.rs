//! Tests for the gRPC service specification generator.

use indexmap::IndexMap;
use typeway_grpc::spec::*;

// ---------------------------------------------------------------------------
// Helper: build a spec manually for unit testing
// ---------------------------------------------------------------------------

fn sample_spec() -> GrpcServiceSpec {
    let mut methods = IndexMap::new();
    methods.insert(
        "ListUser".to_string(),
        MethodSpec {
            name: "ListUser".to_string(),
            full_path: "/users.v1.UserService/ListUser".to_string(),
            rest_path: "/users".to_string(),
            http_method: "GET".to_string(),
            request_type: "google.protobuf.Empty".to_string(),
            response_type: "ListUserResponse".to_string(),
            server_streaming: false,
            client_streaming: false,
            description: None,
            summary: None,
            tags: Vec::new(),
            requires_auth: false,
        },
    );
    methods.insert(
        "GetUser".to_string(),
        MethodSpec {
            name: "GetUser".to_string(),
            full_path: "/users.v1.UserService/GetUser".to_string(),
            rest_path: "/users/{}".to_string(),
            http_method: "GET".to_string(),
            request_type: "GetUserRequest".to_string(),
            response_type: "User".to_string(),
            server_streaming: false,
            client_streaming: false,
            description: Some("Fetch a single user by ID".to_string()),
            summary: Some("Get user".to_string()),
            tags: vec!["users".to_string()],
            requires_auth: true,
        },
    );
    methods.insert(
        "CreateUser".to_string(),
        MethodSpec {
            name: "CreateUser".to_string(),
            full_path: "/users.v1.UserService/CreateUser".to_string(),
            rest_path: "/users".to_string(),
            http_method: "POST".to_string(),
            request_type: "CreateUserRequest".to_string(),
            response_type: "User".to_string(),
            server_streaming: false,
            client_streaming: false,
            description: None,
            summary: None,
            tags: Vec::new(),
            requires_auth: false,
        },
    );

    let mut messages = IndexMap::new();
    messages.insert(
        "User".to_string(),
        MessageSpec {
            name: "User".to_string(),
            fields: vec![
                FieldSpec {
                    name: "id".to_string(),
                    proto_type: "uint32".to_string(),
                    tag: 1,
                    repeated: false,
                    optional: false,
                    is_map: false,
                    map_key_type: None,
                    map_value_type: None,
                    description: None,
                },
                FieldSpec {
                    name: "name".to_string(),
                    proto_type: "string".to_string(),
                    tag: 2,
                    repeated: false,
                    optional: false,
                    is_map: false,
                    map_key_type: None,
                    map_value_type: None,
                    description: None,
                },
            ],
            description: None,
        },
    );
    messages.insert(
        "GetUserRequest".to_string(),
        MessageSpec {
            name: "GetUserRequest".to_string(),
            fields: vec![FieldSpec {
                name: "param1".to_string(),
                proto_type: "string".to_string(),
                tag: 1,
                repeated: false,
                optional: false,
                is_map: false,
                map_key_type: None,
                map_value_type: None,
                description: None,
            }],
            description: None,
        },
    );

    GrpcServiceSpec {
        proto: "syntax = \"proto3\";\npackage users.v1;\nservice UserService {}".to_string(),
        service: ServiceInfo {
            name: "UserService".to_string(),
            package: "users.v1".to_string(),
            full_name: "users.v1.UserService".to_string(),
            description: Some("User management service".to_string()),
            version: Some("1.0.0".to_string()),
        },
        methods,
        messages,
    }
}

// ---------------------------------------------------------------------------
// Tests for GrpcServiceSpec structure
// ---------------------------------------------------------------------------

#[test]
fn spec_has_correct_service_info() {
    let spec = sample_spec();
    assert_eq!(spec.service.name, "UserService");
    assert_eq!(spec.service.package, "users.v1");
    assert_eq!(spec.service.full_name, "users.v1.UserService");
    assert_eq!(
        spec.service.description.as_deref(),
        Some("User management service")
    );
    assert_eq!(spec.service.version.as_deref(), Some("1.0.0"));
}

#[test]
fn spec_has_all_methods() {
    let spec = sample_spec();
    assert_eq!(spec.methods.len(), 3);
    assert!(spec.methods.contains_key("ListUser"));
    assert!(spec.methods.contains_key("GetUser"));
    assert!(spec.methods.contains_key("CreateUser"));
}

#[test]
fn spec_has_message_definitions() {
    let spec = sample_spec();
    assert_eq!(spec.messages.len(), 2);
    assert!(spec.messages.contains_key("User"));
    assert!(spec.messages.contains_key("GetUserRequest"));

    let user = &spec.messages["User"];
    assert_eq!(user.fields.len(), 2);
    assert_eq!(user.fields[0].name, "id");
    assert_eq!(user.fields[1].name, "name");
}

#[test]
fn spec_serializes_to_json() {
    let spec = sample_spec();
    let json = serde_json::to_string(&spec).expect("serialization");
    let roundtrip: GrpcServiceSpec = serde_json::from_str(&json).expect("deserialization");

    assert_eq!(roundtrip.service.name, spec.service.name);
    assert_eq!(roundtrip.methods.len(), spec.methods.len());
    assert_eq!(roundtrip.messages.len(), spec.messages.len());
}

#[test]
fn spec_json_roundtrip_preserves_all_fields() {
    let spec = sample_spec();
    let json = serde_json::to_string_pretty(&spec).unwrap();
    let roundtrip: GrpcServiceSpec = serde_json::from_str(&json).unwrap();

    // Check method details survive roundtrip.
    let get_user = &roundtrip.methods["GetUser"];
    assert_eq!(get_user.full_path, "/users.v1.UserService/GetUser");
    assert_eq!(get_user.rest_path, "/users/{}");
    assert_eq!(get_user.http_method, "GET");
    assert_eq!(get_user.request_type, "GetUserRequest");
    assert_eq!(get_user.response_type, "User");
    assert!(!get_user.server_streaming);
    assert!(!get_user.client_streaming);
    assert!(get_user.requires_auth);
    assert_eq!(get_user.tags, vec!["users"]);
    assert_eq!(get_user.summary.as_deref(), Some("Get user"));
    assert_eq!(
        get_user.description.as_deref(),
        Some("Fetch a single user by ID")
    );
}

#[test]
fn spec_with_docs_applies_handler_docs() {
    let mut spec = sample_spec();

    // Simulate applying handler docs to methods.
    // The ApiToGrpcSpec trait does PascalCase matching; here we test the
    // data flow by directly matching method names.
    let doc = typeway_core::HandlerDoc {
        summary: "List all users in the system",
        description: "Returns a paginated list of users.",
        operation_id: "list_user",
        tags: &["users", "admin"],
    };

    // Apply using the same PascalCase logic.
    let pascal = typeway_grpc::spec::to_pascal_case(doc.operation_id);
    assert_eq!(pascal, "ListUser");

    if let Some(method) = spec.methods.get_mut(&pascal) {
        method.summary = Some(doc.summary.to_string());
        method.description = Some(doc.description.to_string());
        method.tags = doc.tags.iter().map(|s| s.to_string()).collect();
    }

    let list_user = &spec.methods["ListUser"];
    assert_eq!(
        list_user.summary.as_deref(),
        Some("List all users in the system")
    );
    assert_eq!(
        list_user.description.as_deref(),
        Some("Returns a paginated list of users.")
    );
    assert_eq!(list_user.tags, vec!["users", "admin"]);
}

// ---------------------------------------------------------------------------
// Tests for HTML documentation generation
// ---------------------------------------------------------------------------

#[test]
fn docs_html_contains_service_name() {
    let spec = sample_spec();
    let html = typeway_grpc::generate_docs_html(&spec);
    assert!(html.contains("UserService"));
}

#[test]
fn docs_html_contains_methods() {
    let spec = sample_spec();
    let html = typeway_grpc::generate_docs_html(&spec);
    assert!(html.contains("ListUser"));
    assert!(html.contains("GetUser"));
    assert!(html.contains("CreateUser"));
}

#[test]
fn docs_html_contains_proto() {
    let spec = sample_spec();
    let html = typeway_grpc::generate_docs_html(&spec);
    assert!(html.contains("proto3"));
    assert!(html.contains("service UserService"));
}

#[test]
fn docs_html_contains_message_definitions() {
    let spec = sample_spec();
    let html = typeway_grpc::generate_docs_html(&spec);
    assert!(html.contains("uint32"));
    assert!(html.contains("string"));
}

#[test]
fn docs_html_is_valid_html() {
    let spec = sample_spec();
    let html = typeway_grpc::generate_docs_html(&spec);
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("</html>"));
}
