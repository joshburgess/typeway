//! JSON-to-protobuf binary transcoding for the gRPC bridge.
//!
//! When the `proto-binary` feature is enabled, the bridge can serve both
//! `application/grpc` (binary protobuf) and `application/grpc+json` (JSON)
//! requests. Standard gRPC clients use binary protobuf by default, so this
//! module enables interop without requiring clients to switch to JSON mode.
//!
//! Transcoding is driven by the [`GrpcServiceSpec`](crate::spec::GrpcServiceSpec)
//! generated at startup from the API type. The spec provides message field
//! definitions which the [`ProtoTranscoder`] uses to convert between JSON and
//! protobuf binary at the wire level.
//!
//! # Architecture
//!
//! ```text
//! Standard gRPC client
//!   |  (application/grpc, binary protobuf)
//!   v
//! GrpcBridge / Multiplexer
//!   |  ProtoTranscoder::decode_request(proto_bytes) → JSON
//!   v
//! REST handler (processes JSON as usual)
//!   |  JSON response
//!   v
//! ProtoTranscoder::encode_response(json) → proto_bytes
//!   |  (application/grpc, binary protobuf)
//!   v
//! Standard gRPC client
//! ```

use crate::proto_codec::{self, CodecError, ProtoFieldDef};
use crate::spec::{FieldSpec, GrpcServiceSpec, MethodSpec};

/// A transcoder that converts between JSON and protobuf binary
/// using field definitions from a [`GrpcServiceSpec`].
///
/// Construct one from a spec at server startup, then call
/// [`decode_request`](Self::decode_request) and
/// [`encode_response`](Self::encode_response) in the bridge hot path.
///
/// The transcoder caches the field definitions for each message type
/// to avoid repeated lookups.
#[derive(Debug, Clone)]
pub struct ProtoTranscoder {
    spec: GrpcServiceSpec,
}

/// Errors from transcoding operations.
#[derive(Debug, Clone)]
pub enum TranscodeError {
    /// The specified gRPC method was not found in the service spec.
    MethodNotFound(String),
    /// The specified message type was not found in the service spec.
    MessageNotFound(String),
    /// Protobuf binary encoding or decoding failed.
    Codec(CodecError),
    /// JSON serialization or deserialization failed.
    Json(String),
}

impl std::fmt::Display for TranscodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MethodNotFound(m) => write!(f, "method not found in spec: {m}"),
            Self::MessageNotFound(m) => write!(f, "message not found in spec: {m}"),
            Self::Codec(e) => write!(f, "proto codec error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
        }
    }
}

impl std::error::Error for TranscodeError {}

impl From<CodecError> for TranscodeError {
    fn from(e: CodecError) -> Self {
        Self::Codec(e)
    }
}

impl ProtoTranscoder {
    /// Create a transcoder from a gRPC service specification.
    ///
    /// The spec provides message definitions for all request and response
    /// types, which the transcoder uses for encoding and decoding.
    pub fn new(spec: GrpcServiceSpec) -> Self {
        ProtoTranscoder { spec }
    }

    /// Look up the [`MethodSpec`] for a gRPC method by its full path.
    ///
    /// The path should be in the form `/package.Service/MethodName`.
    pub fn find_method(&self, full_path: &str) -> Option<&MethodSpec> {
        self.spec.methods.values().find(|m| m.full_path == full_path)
    }

    /// Get the field definitions for a message type.
    ///
    /// Returns `None` if the message is not found in the spec or is
    /// `google.protobuf.Empty`.
    pub fn message_fields(&self, message_name: &str) -> Option<Vec<ProtoFieldDef>> {
        if message_name == "google.protobuf.Empty" {
            return Some(Vec::new());
        }
        let msg = self.spec.messages.get(message_name)?;
        Some(field_specs_to_defs(&msg.fields, &self.spec))
    }

    /// Decode a protobuf binary request body into JSON.
    ///
    /// Looks up the request message type for the given gRPC method and
    /// uses its field definitions to decode the binary data.
    ///
    /// Returns an empty JSON object `{}` if the request type is
    /// `google.protobuf.Empty` or the binary data is empty.
    pub fn decode_request(
        &self,
        grpc_method_path: &str,
        proto_bytes: &[u8],
    ) -> Result<serde_json::Value, TranscodeError> {
        let method = self
            .find_method(grpc_method_path)
            .ok_or_else(|| TranscodeError::MethodNotFound(grpc_method_path.to_string()))?;

        let request_type = &method.request_type;
        if request_type == "google.protobuf.Empty" || proto_bytes.is_empty() {
            return Ok(serde_json::json!({}));
        }

        let fields = self
            .message_fields(request_type)
            .ok_or_else(|| TranscodeError::MessageNotFound(request_type.clone()))?;

        let json = proto_codec::proto_binary_to_json(proto_bytes, &fields)?;
        Ok(json)
    }

    /// Encode a JSON response body as protobuf binary.
    ///
    /// Looks up the response message type for the given gRPC method and
    /// uses its field definitions to encode the JSON value.
    ///
    /// Returns empty bytes if the response type is `google.protobuf.Empty`.
    pub fn encode_response(
        &self,
        grpc_method_path: &str,
        json: &serde_json::Value,
    ) -> Result<Vec<u8>, TranscodeError> {
        let method = self
            .find_method(grpc_method_path)
            .ok_or_else(|| TranscodeError::MethodNotFound(grpc_method_path.to_string()))?;

        let response_type = &method.response_type;
        if response_type == "google.protobuf.Empty" {
            return Ok(Vec::new());
        }

        let fields = self
            .message_fields(response_type)
            .ok_or_else(|| TranscodeError::MessageNotFound(response_type.clone()))?;

        let bytes = proto_codec::json_to_proto_binary(json, &fields)?;
        Ok(bytes)
    }

    /// Encode a single JSON value as protobuf binary for a named message type.
    ///
    /// Useful for encoding individual items in a server-streaming response.
    pub fn encode_message(
        &self,
        message_name: &str,
        json: &serde_json::Value,
    ) -> Result<Vec<u8>, TranscodeError> {
        if message_name == "google.protobuf.Empty" {
            return Ok(Vec::new());
        }

        let fields = self
            .message_fields(message_name)
            .ok_or_else(|| TranscodeError::MessageNotFound(message_name.to_string()))?;

        let bytes = proto_codec::json_to_proto_binary(json, &fields)?;
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------------
// FieldSpec → ProtoFieldDef conversion
// ---------------------------------------------------------------------------

/// Convert a slice of [`FieldSpec`] (from the gRPC spec) into
/// [`ProtoFieldDef`] (for the codec), resolving nested message fields
/// by looking them up in the spec.
fn field_specs_to_defs(fields: &[FieldSpec], spec: &GrpcServiceSpec) -> Vec<ProtoFieldDef> {
    fields
        .iter()
        .map(|f| {
            let nested = if !is_scalar(&f.proto_type) && !f.is_map {
                // Look up nested message fields in the spec.
                spec.messages
                    .get(&f.proto_type)
                    .map(|msg| field_specs_to_defs(&msg.fields, spec))
            } else {
                None
            };

            ProtoFieldDef {
                name: f.name.clone(),
                proto_type: f.proto_type.clone(),
                tag: f.tag,
                repeated: f.repeated,
                is_map: f.is_map,
                map_key_type: f.map_key_type.clone(),
                map_value_type: f.map_value_type.clone(),
                nested_fields: nested,
            }
        })
        .collect()
}

/// Return `true` if the proto type name is a scalar (non-message) type.
fn is_scalar(proto_type: &str) -> bool {
    matches!(
        proto_type,
        "int32"
            | "int64"
            | "uint32"
            | "uint64"
            | "sint32"
            | "sint64"
            | "bool"
            | "enum"
            | "double"
            | "fixed64"
            | "sfixed64"
            | "string"
            | "bytes"
            | "float"
            | "fixed32"
            | "sfixed32"
            | "google.protobuf.Empty"
    )
}

/// Detect whether a request uses binary protobuf encoding based on
/// the `content-type` header.
///
/// Returns `true` for `application/grpc` and `application/grpc+proto`.
/// Returns `false` for `application/grpc+json` and anything else.
pub fn is_proto_binary_content_type(content_type: &str) -> bool {
    // `application/grpc` (no suffix) defaults to proto binary.
    // `application/grpc+proto` is explicitly proto binary.
    // `application/grpc+json` is JSON.
    content_type == "application/grpc"
        || content_type == "application/grpc+proto"
}

/// Return `true` if the content-type indicates gRPC JSON encoding.
pub fn is_grpc_json_content_type(content_type: &str) -> bool {
    content_type == "application/grpc+json"
}

/// Extract the content-type from request headers, defaulting to
/// `"application/grpc"` for gRPC requests without an explicit content-type.
pub fn grpc_content_type(headers: &http::HeaderMap) -> &str {
    headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/grpc")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{FieldSpec, GrpcServiceSpec, MessageSpec, MethodSpec, ServiceInfo};
    use indexmap::IndexMap;

    fn test_spec() -> GrpcServiceSpec {
        let mut methods = IndexMap::new();
        methods.insert(
            "GetUser".to_string(),
            MethodSpec {
                name: "GetUser".to_string(),
                full_path: "/test.v1.Svc/GetUser".to_string(),
                rest_path: "/users/{}".to_string(),
                http_method: "GET".to_string(),
                request_type: "GetUserRequest".to_string(),
                response_type: "User".to_string(),
                server_streaming: false,
                client_streaming: false,
                description: None,
                summary: None,
                tags: Vec::new(),
                requires_auth: false,
            },
        );
        methods.insert(
            "ListUser".to_string(),
            MethodSpec {
                name: "ListUser".to_string(),
                full_path: "/test.v1.Svc/ListUser".to_string(),
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

        let mut messages = IndexMap::new();
        messages.insert(
            "GetUserRequest".to_string(),
            MessageSpec {
                name: "GetUserRequest".to_string(),
                fields: vec![FieldSpec {
                    name: "id".to_string(),
                    proto_type: "uint32".to_string(),
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
            "ListUserResponse".to_string(),
            MessageSpec {
                name: "ListUserResponse".to_string(),
                fields: vec![FieldSpec {
                    name: "users".to_string(),
                    proto_type: "User".to_string(),
                    tag: 1,
                    repeated: true,
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
            proto: String::new(),
            service: ServiceInfo {
                name: "Svc".to_string(),
                package: "test.v1".to_string(),
                full_name: "test.v1.Svc".to_string(),
                description: None,
                version: None,
            },
            methods,
            messages,
        }
    }

    #[test]
    fn decode_request_simple() {
        let spec = test_spec();
        let tc = ProtoTranscoder::new(spec);

        // Encode a GetUserRequest with id=42.
        let fields = tc.message_fields("GetUserRequest").unwrap();
        let json_in = serde_json::json!({"id": 42});
        let proto_bytes = proto_codec::json_to_proto_binary(&json_in, &fields).unwrap();

        // Decode via the transcoder.
        let json_out = tc.decode_request("/test.v1.Svc/GetUser", &proto_bytes).unwrap();
        assert_eq!(json_out["id"], 42);
    }

    #[test]
    fn encode_response_simple() {
        let spec = test_spec();
        let tc = ProtoTranscoder::new(spec);

        let json = serde_json::json!({"id": 1, "name": "Alice"});
        let proto_bytes = tc.encode_response("/test.v1.Svc/GetUser", &json).unwrap();

        // Decode it back.
        let fields = tc.message_fields("User").unwrap();
        let decoded = proto_codec::proto_binary_to_json(&proto_bytes, &fields).unwrap();
        assert_eq!(decoded["id"], 1);
        assert_eq!(decoded["name"], "Alice");
    }

    #[test]
    fn decode_empty_request() {
        let spec = test_spec();
        let tc = ProtoTranscoder::new(spec);

        // ListUser has Empty request type.
        let result = tc.decode_request("/test.v1.Svc/ListUser", &[]).unwrap();
        assert_eq!(result, serde_json::json!({}));
    }

    #[test]
    fn method_not_found() {
        let spec = test_spec();
        let tc = ProtoTranscoder::new(spec);

        let result = tc.decode_request("/test.v1.Svc/DeleteUser", &[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TranscodeError::MethodNotFound(_)));
    }

    #[test]
    fn content_type_detection() {
        assert!(is_proto_binary_content_type("application/grpc"));
        assert!(is_proto_binary_content_type("application/grpc+proto"));
        assert!(!is_proto_binary_content_type("application/grpc+json"));
        assert!(!is_proto_binary_content_type("application/json"));

        assert!(is_grpc_json_content_type("application/grpc+json"));
        assert!(!is_grpc_json_content_type("application/grpc"));
    }

    #[test]
    fn roundtrip_request_response() {
        let spec = test_spec();
        let tc = ProtoTranscoder::new(spec);

        // Simulate a full request-response cycle.
        // 1. Client sends binary proto request.
        let req_json = serde_json::json!({"id": 7});
        let req_fields = tc.message_fields("GetUserRequest").unwrap();
        let req_proto = proto_codec::json_to_proto_binary(&req_json, &req_fields).unwrap();

        // 2. Bridge decodes to JSON for the handler.
        let handler_input = tc.decode_request("/test.v1.Svc/GetUser", &req_proto).unwrap();
        assert_eq!(handler_input["id"], 7);

        // 3. Handler returns JSON.
        let handler_output = serde_json::json!({"id": 7, "name": "Bob"});

        // 4. Bridge encodes response to binary proto.
        let resp_proto = tc.encode_response("/test.v1.Svc/GetUser", &handler_output).unwrap();

        // 5. Client decodes binary proto response.
        let resp_fields = tc.message_fields("User").unwrap();
        let final_output = proto_codec::proto_binary_to_json(&resp_proto, &resp_fields).unwrap();
        assert_eq!(final_output["id"], 7);
        assert_eq!(final_output["name"], "Bob");
    }
}
