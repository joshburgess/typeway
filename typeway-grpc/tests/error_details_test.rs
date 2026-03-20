use std::collections::HashMap;
use std::time::Duration;

use typeway_grpc::error_details::*;
use typeway_grpc::GrpcCode;

#[test]
fn rich_status_serializes_to_json() {
    let status = RichGrpcStatus::new(GrpcCode::InvalidArgument, "validation failed")
        .with_bad_request(BadRequest {
            field_violations: vec![FieldViolation {
                field: "email".to_string(),
                description: "must contain @".to_string(),
            }],
        });

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    assert_eq!(json["code"], 3);
    assert_eq!(json["message"], "validation failed");
    assert!(json["details"].is_array());
    assert_eq!(json["details"].as_array().unwrap().len(), 1);
}

#[test]
fn bad_request_has_field_violations() {
    let status = RichGrpcStatus::new(GrpcCode::InvalidArgument, "bad input")
        .with_bad_request(BadRequest {
            field_violations: vec![
                FieldViolation {
                    field: "email".to_string(),
                    description: "must contain @".to_string(),
                },
                FieldViolation {
                    field: "password".to_string(),
                    description: "must be at least 8 characters".to_string(),
                },
            ],
        });

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let detail = &json["details"][0];
    assert_eq!(
        detail["@type"],
        "type.googleapis.com/google.rpc.BadRequest"
    );
    let violations = detail["field_violations"].as_array().unwrap();
    assert_eq!(violations.len(), 2);
    assert_eq!(violations[0]["field"], "email");
    assert_eq!(violations[0]["description"], "must contain @");
    assert_eq!(violations[1]["field"], "password");
}

#[test]
fn retry_info_serializes() {
    let status = RichGrpcStatus::new(GrpcCode::Unavailable, "try again later")
        .with_retry_info(Duration::new(5, 500_000_000));

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let detail = &json["details"][0];
    assert_eq!(detail["@type"], "type.googleapis.com/google.rpc.RetryInfo");
    assert_eq!(detail["retry_delay_seconds"], 5);
    assert_eq!(detail["retry_delay_nanos"], 500_000_000);
}

#[test]
fn debug_info_serializes() {
    let status = RichGrpcStatus::new(GrpcCode::Internal, "internal error").with_debug_info(
        vec![
            "main.rs:42".to_string(),
            "handler.rs:17".to_string(),
        ],
        "null pointer in user lookup",
    );

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let detail = &json["details"][0];
    assert_eq!(detail["@type"], "type.googleapis.com/google.rpc.DebugInfo");
    let entries = detail["stack_entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0], "main.rs:42");
    assert_eq!(detail["detail"], "null pointer in user lookup");
}

#[test]
fn error_info_with_metadata() {
    let mut metadata = HashMap::new();
    metadata.insert("consumer".to_string(), "projects/123".to_string());

    let status = RichGrpcStatus::new(GrpcCode::ResourceExhausted, "rate limited")
        .with_error_info("RATE_LIMIT_EXCEEDED", "googleapis.com", metadata);

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let detail = &json["details"][0];
    assert_eq!(detail["@type"], "type.googleapis.com/google.rpc.ErrorInfo");
    assert_eq!(detail["reason"], "RATE_LIMIT_EXCEEDED");
    assert_eq!(detail["domain"], "googleapis.com");
    assert_eq!(detail["metadata"]["consumer"], "projects/123");
}

#[test]
fn multiple_details() {
    let status = RichGrpcStatus::new(GrpcCode::InvalidArgument, "multiple issues")
        .with_bad_request(BadRequest {
            field_violations: vec![FieldViolation {
                field: "name".to_string(),
                description: "required".to_string(),
            }],
        })
        .with_debug_info(vec![], "extra context")
        .with_help(vec![HelpLink {
            description: "API docs".to_string(),
            url: "https://example.com/docs".to_string(),
        }]);

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let details = json["details"].as_array().unwrap();
    assert_eq!(details.len(), 3);
    assert_eq!(
        details[0]["@type"],
        "type.googleapis.com/google.rpc.BadRequest"
    );
    assert_eq!(
        details[1]["@type"],
        "type.googleapis.com/google.rpc.DebugInfo"
    );
    assert_eq!(
        details[2]["@type"],
        "type.googleapis.com/google.rpc.Help"
    );
}

#[test]
fn to_grpc_response_parts_has_headers_and_body() {
    let status = RichGrpcStatus::new(GrpcCode::NotFound, "user not found")
        .with_resource_info("user", "user-42", "", "no user with id 42");

    let (headers, body) = status.to_grpc_response_parts();

    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0], ("grpc-status".to_string(), "5".to_string()));
    assert_eq!(
        headers[1],
        ("grpc-message".to_string(), "user not found".to_string())
    );
    assert!(!body.is_empty());

    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], 5);
    assert!(json["details"].is_array());
}

#[test]
fn empty_details_omitted() {
    let status = RichGrpcStatus::new(GrpcCode::Internal, "oops");

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    // When details is empty, the field should be omitted from JSON.
    assert!(json.get("details").is_none());

    // Body should be empty when there are no details.
    let (_, body) = status.to_grpc_response_parts();
    assert!(body.is_empty());
}

#[test]
fn quota_failure_serializes() {
    let status = RichGrpcStatus::new(GrpcCode::ResourceExhausted, "quota exceeded")
        .with_quota_failure(vec![QuotaViolation {
            subject: "project:my-project".to_string(),
            description: "API calls per minute exceeded".to_string(),
        }]);

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let detail = &json["details"][0];
    assert_eq!(
        detail["@type"],
        "type.googleapis.com/google.rpc.QuotaFailure"
    );
    let violations = detail["violations"].as_array().unwrap();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0]["subject"], "project:my-project");
    assert_eq!(
        violations[0]["description"],
        "API calls per minute exceeded"
    );
}

#[test]
fn precondition_failure_serializes() {
    let status = RichGrpcStatus::new(GrpcCode::InvalidArgument, "precondition failed")
        .with_precondition_failure(vec![PreconditionViolation {
            violation_type: "TOS".to_string(),
            subject: "google.com/terms".to_string(),
            description: "Terms of Service must be accepted".to_string(),
        }]);

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let detail = &json["details"][0];
    assert_eq!(
        detail["@type"],
        "type.googleapis.com/google.rpc.PreconditionFailure"
    );
    let violations = detail["violations"].as_array().unwrap();
    assert_eq!(violations[0]["type"], "TOS");
    assert_eq!(violations[0]["subject"], "google.com/terms");
}

#[test]
fn resource_info_serializes() {
    let status = RichGrpcStatus::new(GrpcCode::NotFound, "not found").with_resource_info(
        "user",
        "user-42",
        "admin@example.com",
        "User with ID 42 was not found",
    );

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let detail = &json["details"][0];
    assert_eq!(
        detail["@type"],
        "type.googleapis.com/google.rpc.ResourceInfo"
    );
    assert_eq!(detail["resource_type"], "user");
    assert_eq!(detail["resource_name"], "user-42");
    assert_eq!(detail["owner"], "admin@example.com");
    assert_eq!(detail["description"], "User with ID 42 was not found");
}

#[test]
fn help_links_serialize() {
    let status = RichGrpcStatus::new(GrpcCode::InvalidArgument, "see docs").with_help(vec![
        HelpLink {
            description: "API documentation".to_string(),
            url: "https://example.com/api".to_string(),
        },
        HelpLink {
            description: "Troubleshooting guide".to_string(),
            url: "https://example.com/troubleshoot".to_string(),
        },
    ]);

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let detail = &json["details"][0];
    assert_eq!(detail["@type"], "type.googleapis.com/google.rpc.Help");
    let links = detail["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!(links[0]["description"], "API documentation");
    assert_eq!(links[0]["url"], "https://example.com/api");
    assert_eq!(links[1]["description"], "Troubleshooting guide");
}

#[test]
fn localized_message_serializes() {
    let status = RichGrpcStatus::new(GrpcCode::InvalidArgument, "validation error")
        .with_localized_message("fr-FR", "L'email est invalide");

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let detail = &json["details"][0];
    assert_eq!(
        detail["@type"],
        "type.googleapis.com/google.rpc.LocalizedMessage"
    );
    assert_eq!(detail["locale"], "fr-FR");
    assert_eq!(detail["message"], "L'email est invalide");
}

#[test]
fn rich_status_json_has_google_type_urls() {
    let status = RichGrpcStatus::new(GrpcCode::Internal, "test")
        .with_bad_request(BadRequest {
            field_violations: vec![],
        })
        .with_retry_info(Duration::from_secs(1))
        .with_debug_info(vec![], "")
        .with_error_info("R", "D", HashMap::new())
        .with_quota_failure(vec![])
        .with_precondition_failure(vec![])
        .with_resource_info("", "", "", "")
        .with_help(vec![])
        .with_localized_message("en", "msg");

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let details = json["details"].as_array().unwrap();
    let expected_type_urls = [
        "type.googleapis.com/google.rpc.BadRequest",
        "type.googleapis.com/google.rpc.RetryInfo",
        "type.googleapis.com/google.rpc.DebugInfo",
        "type.googleapis.com/google.rpc.ErrorInfo",
        "type.googleapis.com/google.rpc.QuotaFailure",
        "type.googleapis.com/google.rpc.PreconditionFailure",
        "type.googleapis.com/google.rpc.ResourceInfo",
        "type.googleapis.com/google.rpc.Help",
        "type.googleapis.com/google.rpc.LocalizedMessage",
    ];

    assert_eq!(details.len(), expected_type_urls.len());
    for (detail, expected_url) in details.iter().zip(expected_type_urls.iter()) {
        assert_eq!(detail["@type"].as_str().unwrap(), *expected_url);
    }
}

#[test]
fn grpc_status_into_rich_preserves_code_and_message() {
    let status = typeway_grpc::GrpcStatus::not_found("user 42 not found");
    let rich = status.into_rich();
    assert_eq!(rich.code, 5);
    assert_eq!(rich.message, "user 42 not found");
    assert!(rich.details.is_empty());
}

#[test]
fn grpc_status_into_rich_then_add_details() {
    let rich = typeway_grpc::GrpcStatus::invalid_argument("bad input")
        .into_rich()
        .with_bad_request(BadRequest {
            field_violations: vec![FieldViolation {
                field: "email".to_string(),
                description: "required".to_string(),
            }],
        });

    assert_eq!(rich.code, 3);
    assert_eq!(rich.details.len(), 1);
}

#[test]
fn error_info_empty_metadata_omitted_in_json() {
    let status = RichGrpcStatus::new(GrpcCode::Internal, "err")
        .with_error_info("REASON", "domain.com", HashMap::new());

    let json: serde_json::Value = serde_json::from_slice(&status.to_json_bytes()).unwrap();
    let detail = &json["details"][0];
    // Empty metadata should be omitted from serialization.
    assert!(detail.get("metadata").is_none());
}

#[test]
fn rich_status_roundtrip_deserialization() {
    let status = RichGrpcStatus::new(GrpcCode::InvalidArgument, "test")
        .with_bad_request(BadRequest {
            field_violations: vec![FieldViolation {
                field: "name".to_string(),
                description: "required".to_string(),
            }],
        })
        .with_retry_info(Duration::from_millis(1500));

    let bytes = status.to_json_bytes();
    let deserialized: RichGrpcStatus = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(deserialized.code, status.code);
    assert_eq!(deserialized.message, status.message);
    assert_eq!(deserialized.details.len(), 2);
}
