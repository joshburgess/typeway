//! gRPC codec abstraction.
//!
//! [`GrpcCodec`] abstracts over the serialization format used for gRPC
//! messages. Two implementations:
//!
//! - [`JsonCodec`] — JSON encoding (always available, default)
//! - `BinaryCodec` — protobuf binary encoding (behind `proto-binary` feature),
//!   enables standard gRPC clients (grpcurl, tonic, Postman) to interop

use std::fmt;

/// A codec that encodes and decodes gRPC message payloads.
///
/// The codec operates on [`serde_json::Value`] as the intermediate
/// representation. This is natural because Typeway handlers already
/// use JSON via REST extractors — the codec sits between the wire
/// format and this JSON representation.
///
/// # Implementors
///
/// - [`JsonCodec`] — JSON encoding (always available, default)
/// - `ProstCodec` — protobuf binary encoding (Phase 2, behind feature flag)
pub trait GrpcCodec: Send + Sync + 'static {
    /// The gRPC content-type for this codec.
    ///
    /// Examples: `"application/grpc+json"`, `"application/grpc+proto"`.
    fn content_type(&self) -> &'static str;

    /// Encode a JSON value to wire bytes.
    fn encode(&self, value: &serde_json::Value) -> Result<Vec<u8>, CodecError>;

    /// Decode wire bytes into a JSON value.
    fn decode(&self, bytes: &[u8]) -> Result<serde_json::Value, CodecError>;
}

/// JSON codec — the default for typeway-grpc.
///
/// Encodes messages as JSON, matching the `application/grpc+json` content type.
/// This is the default format used by the native dispatch and client macros.
#[derive(Debug, Clone, Copy)]
pub struct JsonCodec;

impl GrpcCodec for JsonCodec {
    fn content_type(&self) -> &'static str {
        "application/grpc+json"
    }

    fn encode(&self, value: &serde_json::Value) -> Result<Vec<u8>, CodecError> {
        serde_json::to_vec(value).map_err(|e| CodecError {
            kind: CodecErrorKind::Encode,
            message: e.to_string(),
        })
    }

    fn decode(&self, bytes: &[u8]) -> Result<serde_json::Value, CodecError> {
        if bytes.is_empty() {
            return Ok(serde_json::Value::Object(Default::default()));
        }
        serde_json::from_slice(bytes).map_err(|e| CodecError {
            kind: CodecErrorKind::Decode,
            message: e.to_string(),
        })
    }
}

/// Error from encoding or decoding a gRPC message.
#[derive(Debug, Clone)]
pub struct CodecError {
    /// Whether the error occurred during encoding or decoding.
    pub kind: CodecErrorKind,
    /// Human-readable error message.
    pub message: String,
}

/// Whether a codec error occurred during encoding or decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecErrorKind {
    Encode,
    Decode,
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            CodecErrorKind::Encode => write!(f, "gRPC encode error: {}", self.message),
            CodecErrorKind::Decode => write!(f, "gRPC decode error: {}", self.message),
        }
    }
}

impl std::error::Error for CodecError {}

// ---------------------------------------------------------------------------
// BinaryCodec — protobuf binary encoding via ProtoTranscoder
// ---------------------------------------------------------------------------

/// Binary protobuf codec for standard gRPC client interop.
///
/// Uses the [`ProtoTranscoder`](crate::transcode::ProtoTranscoder) to
/// transcode between protobuf binary wire format and JSON. This enables
/// standard gRPC clients (grpcurl, tonic, Postman) to communicate with
/// typeway handlers without requiring JSON mode.
///
/// The codec is method-aware: it needs to know which gRPC method is being
/// called to look up the correct message field definitions. Use
/// [`with_method`](Self::with_method) to set the method path before
/// encoding/decoding.
///
/// # Example
///
/// ```ignore
/// use typeway_grpc::codec::BinaryCodec;
/// use typeway_grpc::transcode::ProtoTranscoder;
///
/// let transcoder = ProtoTranscoder::new(spec);
/// let codec = BinaryCodec::new(transcoder)
///     .with_method("/pkg.v1.Svc/GetUser");
///
/// // Decode binary protobuf → JSON
/// let json = codec.decode(&proto_bytes)?;
///
/// // Encode JSON → binary protobuf
/// let bytes = codec.encode(&json_value)?;
/// ```
#[cfg(feature = "proto-binary")]
pub struct BinaryCodec {
    transcoder: std::sync::Arc<crate::transcode::ProtoTranscoder>,
    /// The current gRPC method path (e.g., `/pkg.v1.Svc/GetUser`).
    /// Must be set before encode/decode to resolve message types.
    method_path: Option<String>,
    /// Whether we're encoding/decoding requests or responses.
    direction: CodecDirection,
}

/// Whether the codec is operating on requests or responses.
#[cfg(feature = "proto-binary")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecDirection {
    /// Decoding a request from the client (uses request message type).
    Request,
    /// Encoding a response to the client (uses response message type).
    Response,
}

#[cfg(feature = "proto-binary")]
impl BinaryCodec {
    /// Create a new binary codec from a transcoder.
    pub fn new(transcoder: std::sync::Arc<crate::transcode::ProtoTranscoder>) -> Self {
        BinaryCodec {
            transcoder,
            method_path: None,
            direction: CodecDirection::Request,
        }
    }

    /// Set the gRPC method path for message type resolution.
    pub fn with_method(mut self, path: &str) -> Self {
        self.method_path = Some(path.to_string());
        self
    }

    /// Set the codec direction (request decode or response encode).
    pub fn with_direction(mut self, direction: CodecDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Set the method path (mutable reference variant).
    pub fn set_method(&mut self, path: &str) {
        self.method_path = Some(path.to_string());
    }

    /// Set the direction (mutable reference variant).
    pub fn set_direction(&mut self, direction: CodecDirection) {
        self.direction = direction;
    }
}

#[cfg(feature = "proto-binary")]
impl GrpcCodec for BinaryCodec {
    fn content_type(&self) -> &'static str {
        "application/grpc+proto"
    }

    fn encode(&self, value: &serde_json::Value) -> Result<Vec<u8>, CodecError> {
        let method_path = self.method_path.as_deref().ok_or_else(|| CodecError {
            kind: CodecErrorKind::Encode,
            message: "BinaryCodec: method path not set".to_string(),
        })?;

        match self.direction {
            CodecDirection::Response => self
                .transcoder
                .encode_response(method_path, value)
                .map_err(|e| CodecError {
                    kind: CodecErrorKind::Encode,
                    message: e.to_string(),
                }),
            CodecDirection::Request => {
                // Encoding a request (client-side) — use the request message type.
                // For now, look up the method and encode using the request message fields.
                let method =
                    self.transcoder
                        .find_method(method_path)
                        .ok_or_else(|| CodecError {
                            kind: CodecErrorKind::Encode,
                            message: format!("method not found: {method_path}"),
                        })?;
                let msg_name = method.request_type.clone();
                self.transcoder
                    .encode_message(&msg_name, value)
                    .map_err(|e| CodecError {
                        kind: CodecErrorKind::Encode,
                        message: e.to_string(),
                    })
            }
        }
    }

    fn decode(&self, bytes: &[u8]) -> Result<serde_json::Value, CodecError> {
        let method_path = self.method_path.as_deref().ok_or_else(|| CodecError {
            kind: CodecErrorKind::Decode,
            message: "BinaryCodec: method path not set".to_string(),
        })?;

        match self.direction {
            CodecDirection::Request => {
                self.transcoder
                    .decode_request(method_path, bytes)
                    .map_err(|e| CodecError {
                        kind: CodecErrorKind::Decode,
                        message: e.to_string(),
                    })
            }
            CodecDirection::Response => {
                // Decoding a response (client-side) — use the response message fields.
                let method =
                    self.transcoder
                        .find_method(method_path)
                        .ok_or_else(|| CodecError {
                            kind: CodecErrorKind::Decode,
                            message: format!("method not found: {method_path}"),
                        })?;
                let fields = self
                    .transcoder
                    .message_fields(&method.response_type)
                    .ok_or_else(|| CodecError {
                        kind: CodecErrorKind::Decode,
                        message: format!("message not found: {}", method.response_type),
                    })?;
                crate::proto_codec::proto_binary_to_json(bytes, &fields).map_err(|e| CodecError {
                    kind: CodecErrorKind::Decode,
                    message: e.to_string(),
                })
            }
        }
    }
}

#[cfg(all(test, feature = "proto-binary"))]
mod binary_codec_tests {
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
    fn binary_codec_content_type() {
        let spec = test_spec();
        let tc = std::sync::Arc::new(crate::transcode::ProtoTranscoder::new(spec));
        let codec = BinaryCodec::new(tc);
        assert_eq!(codec.content_type(), "application/grpc+proto");
    }

    #[test]
    fn binary_codec_request_roundtrip() {
        let spec = test_spec();
        let tc = std::sync::Arc::new(crate::transcode::ProtoTranscoder::new(spec));

        // Encode a request as binary.
        let mut codec = BinaryCodec::new(tc.clone());
        codec.set_method("/test.v1.Svc/GetUser");
        codec.set_direction(CodecDirection::Request);

        let json = serde_json::json!({"id": 42});
        let encoded = codec.encode(&json).unwrap();

        // Decode the binary back to JSON.
        let decoded = codec.decode(&encoded).unwrap();
        assert_eq!(decoded["id"], 42);
    }

    #[test]
    fn binary_codec_response_roundtrip() {
        let spec = test_spec();
        let tc = std::sync::Arc::new(crate::transcode::ProtoTranscoder::new(spec));

        // Encode a response as binary.
        let mut codec = BinaryCodec::new(tc.clone());
        codec.set_method("/test.v1.Svc/GetUser");
        codec.set_direction(CodecDirection::Response);

        let json = serde_json::json!({"id": 1, "name": "Alice"});
        let encoded = codec.encode(&json).unwrap();

        // Decode the binary back.
        let decoded = codec.decode(&encoded).unwrap();
        assert_eq!(decoded["id"], 1);
        assert_eq!(decoded["name"], "Alice");
    }

    #[test]
    fn binary_codec_no_method_returns_error() {
        let spec = test_spec();
        let tc = std::sync::Arc::new(crate::transcode::ProtoTranscoder::new(spec));
        let codec = BinaryCodec::new(tc);

        let result = codec.decode(b"");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, CodecErrorKind::Decode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_codec_roundtrip() {
        let codec = JsonCodec;
        let value = serde_json::json!({"id": 42, "name": "Alice"});
        let encoded = codec.encode(&value).unwrap();
        let decoded = codec.decode(&encoded).unwrap();
        assert_eq!(value, decoded);
    }

    #[test]
    fn json_codec_empty_bytes_returns_empty_object() {
        let codec = JsonCodec;
        let decoded = codec.decode(b"").unwrap();
        assert_eq!(decoded, serde_json::json!({}));
    }

    #[test]
    fn json_codec_invalid_bytes() {
        let codec = JsonCodec;
        let result = codec.decode(b"not json");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, CodecErrorKind::Decode);
    }

    #[test]
    fn json_codec_content_type() {
        assert_eq!(JsonCodec.content_type(), "application/grpc+json");
    }
}
