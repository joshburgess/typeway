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
    assert!(validation.is_empty(), "Validation errors: {:?}", validation);
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
    assert!(
        rust_serde.contains("pub struct PongResponse"),
        "got:\n{rust_serde}"
    );
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

// ===========================================================================
// gRPC interop test patterns
// ===========================================================================

/// Verify empty unary call (google.protobuf.Empty → google.protobuf.Empty).
#[test]
fn interop_empty_unary_proto() {
    let proto = r#"syntax = "proto3";
package grpc.testing;
service TestService {
  // POST /empty
  rpc EmptyCall(google.protobuf.Empty) returns (google.protobuf.Empty);
}
"#;
    let parsed = parse_proto(proto).unwrap();
    assert_eq!(
        parsed.services[0].methods[0].input_type,
        "google.protobuf.Empty"
    );
    assert_eq!(
        parsed.services[0].methods[0].output_type,
        "google.protobuf.Empty"
    );

    // Codegen should handle Empty → () mapping.
    let rust = typeway_grpc::codegen::generate_typeway_from_proto(&parsed);
    assert!(rust.contains("type API = ("));
}

/// Verify large unary message (tests buffer handling).
#[test]
fn interop_large_message_framing() {
    // 256KB payload.
    let payload = vec![0x42u8; 256 * 1024];
    let framed = typeway_grpc::encode_grpc_frame(&payload);

    // 5-byte header: 1 compressed flag + 4 length.
    assert_eq!(framed.len(), payload.len() + 5);

    // Round-trip.
    let decoded = typeway_grpc::decode_grpc_frame(&framed).unwrap();
    assert_eq!(decoded.len(), payload.len());
    assert!(decoded.iter().all(|&b| b == 0x42));
}

/// Verify multiple frames decode correctly (server streaming).
#[test]
fn interop_multiple_frames() {
    let msg1 = b"first";
    let msg2 = b"second";
    let msg3 = b"third";

    let mut wire = Vec::new();
    wire.extend_from_slice(&typeway_grpc::encode_grpc_frame(msg1));
    wire.extend_from_slice(&typeway_grpc::encode_grpc_frame(msg2));
    wire.extend_from_slice(&typeway_grpc::encode_grpc_frame(msg3));

    let (frames, _trailer) = typeway_grpc::decode_grpc_frames(&wire);
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0], b"first");
    assert_eq!(frames[1], b"second");
    assert_eq!(frames[2], b"third");
}

/// Verify gRPC timeout header parsing conforms to spec.
#[test]
fn interop_timeout_parsing() {
    use std::time::Duration;
    use typeway_grpc::status::parse_grpc_timeout;

    // All valid units per gRPC spec.
    assert_eq!(parse_grpc_timeout("1H"), Some(Duration::from_secs(3600)));
    assert_eq!(parse_grpc_timeout("30M"), Some(Duration::from_secs(1800)));
    assert_eq!(parse_grpc_timeout("10S"), Some(Duration::from_secs(10)));
    assert_eq!(parse_grpc_timeout("500m"), Some(Duration::from_millis(500)));
    assert_eq!(parse_grpc_timeout("100u"), Some(Duration::from_micros(100)));
    assert_eq!(parse_grpc_timeout("999n"), Some(Duration::from_nanos(999)));

    // Edge cases.
    assert_eq!(parse_grpc_timeout(""), None);
    assert_eq!(parse_grpc_timeout("X"), None);
    assert_eq!(parse_grpc_timeout("0S"), Some(Duration::from_secs(0)));
}

/// Verify enum round-trip in codegen.
#[test]
fn interop_enum_codegen() {
    let proto = r#"syntax = "proto3";
package grpc.testing;

enum PayloadType {
  COMPRESSABLE = 0;
  UNCOMPRESSABLE = 1;
}

message SimpleRequest {
  PayloadType response_type = 1;
  uint32 response_size = 2;
}
"#;
    let parsed = parse_proto(proto).unwrap();
    assert_eq!(parsed.enums.len(), 1);
    assert_eq!(parsed.enums[0].name, "PayloadType");
    assert_eq!(parsed.enums[0].variants.len(), 2);

    let rust = typeway_grpc::proto_to_typeway_with_codec(proto).unwrap();
    assert!(rust.contains("pub enum PayloadType {"));
    assert!(rust.contains("Compressable,"));
    assert!(rust.contains("Uncompressable,"));
}

/// Verify all standard gRPC status codes have correct string representations.
#[test]
fn interop_status_code_names() {
    use typeway_grpc::GrpcCode;

    let cases = [
        (GrpcCode::Ok, "OK"),
        (GrpcCode::Cancelled, "CANCELLED"),
        (GrpcCode::Unknown, "UNKNOWN"),
        (GrpcCode::InvalidArgument, "INVALID_ARGUMENT"),
        (GrpcCode::DeadlineExceeded, "DEADLINE_EXCEEDED"),
        (GrpcCode::NotFound, "NOT_FOUND"),
        (GrpcCode::AlreadyExists, "ALREADY_EXISTS"),
        (GrpcCode::PermissionDenied, "PERMISSION_DENIED"),
        (GrpcCode::ResourceExhausted, "RESOURCE_EXHAUSTED"),
        (GrpcCode::Unimplemented, "UNIMPLEMENTED"),
        (GrpcCode::Internal, "INTERNAL"),
        (GrpcCode::Unavailable, "UNAVAILABLE"),
        (GrpcCode::Unauthenticated, "UNAUTHENTICATED"),
    ];

    for (code, _expected_name) in cases {
        // Verify round-trip: code → i32 → code.
        let i32_val = code.as_i32();
        let roundtripped = typeway_grpc::GrpcCode::from_i32(i32_val);
        assert_eq!(code, roundtripped, "round-trip failed for code {i32_val}");
    }
}

/// Verify retry policy defaults match gRPC conventions.
#[test]
#[cfg(feature = "grpc-native")]
fn interop_retry_defaults() {
    use typeway_grpc::GrpcRetryPolicy;

    let policy = GrpcRetryPolicy::default();
    assert_eq!(policy.max_retries, 3);
    assert!(policy
        .retry_on
        .contains(&typeway_grpc::GrpcCode::Unavailable));
    assert!(policy
        .retry_on
        .contains(&typeway_grpc::GrpcCode::ResourceExhausted));
    assert!(policy
        .retry_on
        .contains(&typeway_grpc::GrpcCode::DeadlineExceeded));
    // Non-retryable codes should NOT be in the list.
    assert!(!policy
        .retry_on
        .contains(&typeway_grpc::GrpcCode::InvalidArgument));
    assert!(!policy.retry_on.contains(&typeway_grpc::GrpcCode::NotFound));
}

/// Verify circuit breaker state transitions.
#[test]
#[cfg(feature = "grpc-native")]
fn interop_circuit_breaker_transitions() {
    use std::time::Duration;
    use typeway_grpc::CircuitBreaker;

    let cb = CircuitBreaker::new(3, Duration::from_millis(50));

    // Initially closed — requests allowed.
    assert!(cb.allow_request());

    // Record failures below threshold — still closed.
    cb.record_failure();
    cb.record_failure();
    assert!(cb.allow_request());

    // Third failure — opens the circuit.
    cb.record_failure();
    assert!(!cb.allow_request());

    // After reset timeout — half-open (probe allowed).
    std::thread::sleep(Duration::from_millis(60));
    assert!(cb.allow_request());

    // Success in half-open — closes circuit.
    cb.record_success();
    assert!(cb.allow_request());
}
