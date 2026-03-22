//! gRPC conformance smoke tests.
//!
//! These tests verify that typeway's gRPC server produces well-formed
//! responses that standard gRPC tooling can parse.

use typeway_grpc::proto_parse::parse_proto;

/// Verify that generated .proto files are valid proto3 syntax.
#[test]
fn generated_proto_is_valid_syntax() {
    let proto_source = r#"syntax = "proto3";

package test.v1;

service TestService {
  // GET /items
  rpc ListItems(google.protobuf.Empty) returns (ListItemsResponse);
  // POST /items
  rpc CreateItem(CreateItemRequest) returns (Item);
}

message Item {
  uint32 id = 1;
  string name = 2;
  bool active = 3;
}

message CreateItemRequest {
  string name = 1;
}

message ListItemsResponse {
  repeated Item items = 1;
}
"#;

    let parsed = parse_proto(proto_source).unwrap();
    assert_eq!(parsed.syntax, "proto3");
    assert_eq!(parsed.package, "test.v1");
    assert_eq!(parsed.services.len(), 1);
    assert_eq!(parsed.services[0].methods.len(), 2);
    assert_eq!(parsed.messages.len(), 3);

    // Validate the parsed proto.
    let validation = typeway_grpc::validate::validate_proto(proto_source);
    assert!(
        validation.is_empty(),
        "Validation errors: {:?}",
        validation
    );
}

/// Verify proto round-trip: generate → parse → validate.
#[test]
fn proto_roundtrip_is_stable() {
    let source = r#"syntax = "proto3";

package roundtrip.v1;

service RoundTrip {
  // GET /ping
  rpc Ping(google.protobuf.Empty) returns (PongResponse);
  // POST /echo
  rpc Echo(EchoRequest) returns (EchoResponse);
}

message PongResponse {
  string text = 1;
  uint64 timestamp = 2;
}

message EchoRequest {
  string body = 1;
  repeated string tags = 2;
}

message EchoResponse {
  string body = 1;
}
"#;

    // Parse.
    let proto1 = parse_proto(source).unwrap();
    assert_eq!(proto1.services[0].methods.len(), 2);

    // Validate.
    let errors = typeway_grpc::validate::validate_proto(source);
    assert!(errors.is_empty(), "Validation errors: {:?}", errors);

    // Generate Rust code and verify it's non-empty.
    let rust_serde = typeway_grpc::codegen::generate_typeway_from_proto(&proto1);
    assert!(rust_serde.contains("pub struct PongResponse"), "got:\n{rust_serde}");
    assert!(rust_serde.contains("pub struct EchoRequest"));

    let rust_codec = typeway_grpc::codegen::generate_typeway_from_proto_with_codec(&proto1);
    assert!(rust_codec.contains("TypewayCodec"));
    assert!(rust_codec.contains("BytesStr"));
    assert!(rust_codec.contains("#[proto(tag = 1)]"));
}

/// Verify gRPC framing round-trip.
#[test]
fn grpc_frame_roundtrip() {
    let payload = br#"{"id":1,"name":"test","active":true}"#;

    // Encode frame.
    let framed = typeway_grpc::encode_grpc_frame(payload);
    assert!(framed.len() > payload.len()); // 5-byte header added.

    // Decode frame.
    let decoded = typeway_grpc::decode_grpc_frame(&framed).unwrap();
    assert_eq!(decoded, payload);
}

/// Verify gRPC status codes are well-formed.
#[test]
fn grpc_status_codes_conform() {
    use typeway_grpc::GrpcCode;

    // Standard gRPC codes (0-16).
    let codes = [
        (GrpcCode::Ok, 0),
        (GrpcCode::Cancelled, 1),
        (GrpcCode::Unknown, 2),
        (GrpcCode::InvalidArgument, 3),
        (GrpcCode::DeadlineExceeded, 4),
        (GrpcCode::NotFound, 5),
        (GrpcCode::AlreadyExists, 6),
        (GrpcCode::PermissionDenied, 7),
        (GrpcCode::ResourceExhausted, 8),
        (GrpcCode::Unimplemented, 12),
        (GrpcCode::Internal, 13),
        (GrpcCode::Unavailable, 14),
        (GrpcCode::Unauthenticated, 16),
    ];

    for (code, expected_i32) in codes {
        assert_eq!(code.as_i32(), expected_i32);
        assert_eq!(GrpcCode::from_i32(expected_i32), code);
    }
}

/// Verify rich error details serialize to the expected JSON format.
#[test]
fn rich_error_details_conform_to_google_format() {
    use typeway_grpc::error_details::*;

    let status = RichGrpcStatus::new(typeway_grpc::GrpcCode::InvalidArgument, "validation failed")
        .with_bad_request(BadRequest {
            field_violations: vec![FieldViolation {
                field: "email".to_string(),
                description: "must contain @".to_string(),
            }],
        })
        .with_error_info("INVALID_EMAIL", "myapp.example.com", Default::default());

    let json = status.to_json_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&json).unwrap();

    // Verify structure matches google.rpc.Status.
    assert_eq!(parsed["code"], 3);
    assert_eq!(parsed["message"], "validation failed");

    let details = parsed["details"].as_array().unwrap();
    assert_eq!(details.len(), 2);

    // BadRequest detail has canonical @type URL.
    assert_eq!(
        details[0]["@type"],
        "type.googleapis.com/google.rpc.BadRequest"
    );

    // ErrorInfo detail.
    assert_eq!(
        details[1]["@type"],
        "type.googleapis.com/google.rpc.ErrorInfo"
    );
    assert_eq!(details[1]["reason"], "INVALID_EMAIL");
}

/// Verify rich error details round-trip (serialize → parse).
#[test]
fn rich_error_details_roundtrip() {
    use typeway_grpc::error_details::*;

    let original = RichGrpcStatus::new(typeway_grpc::GrpcCode::NotFound, "user not found")
        .with_resource_info("User", "users/123", "", "No user with ID 123");

    let json = original.to_json_bytes();
    let parsed = parse_rich_status(&json).unwrap();

    assert_eq!(parsed.code, 5); // NOT_FOUND
    assert_eq!(parsed.message, "user not found");
    assert_eq!(parsed.details.len(), 1);

    match &parsed.details[0] {
        ErrorDetail::ResourceInfo(info) => {
            assert_eq!(info.resource_type, "User");
            assert_eq!(info.resource_name, "users/123");
        }
        other => panic!("Expected ResourceInfo, got {:?}", other),
    }
}

/// Verify proto diff detects breaking changes.
#[test]
fn proto_diff_detects_breaking_changes() {
    let old = r#"syntax = "proto3";
package test.v1;
service Svc {
  rpc GetItem(GetItemReq) returns (Item);
  rpc ListItems(google.protobuf.Empty) returns (ListItemsResp);
}
message GetItemReq {
  uint32 id = 1;
}
message Item {
  uint32 id = 1;
  string name = 2;
}
message ListItemsResp {
  repeated Item items = 1;
}
"#;

    let new = r#"syntax = "proto3";
package test.v1;
service Svc {
  rpc GetItem(GetItemReq) returns (Item);
}
message GetItemReq {
  uint32 id = 1;
}
message Item {
  uint32 id = 1;
  string name = 2;
}
"#;

    let changes = typeway_grpc::diff::diff_protos(old, new).unwrap();
    assert!(!changes.is_empty(), "Should detect removed RPC");

    let breaking: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == typeway_grpc::ChangeKind::Breaking)
        .collect();
    assert!(!breaking.is_empty(), "Removing an RPC should be breaking");
}
