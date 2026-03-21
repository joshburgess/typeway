//! The [`Proto<T>`] extractor — format-agnostic protobuf body extraction.
//!
//! `Proto<T>` is like `Json<T>` but auto-detects the wire format:
//! - `application/json` or `application/grpc+json` → serde JSON deserialization
//! - `application/grpc`, `application/grpc+proto`, `application/protobuf`
//!   → `TypewayDecode` binary deserialization (no JSON intermediate)
//!
//! This means the same handler works for REST, gRPC+JSON, and gRPC binary
//! clients without separate implementations.
//!
//! # Example
//!
//! ```ignore
//! use typeway_server::Proto;
//!
//! #[derive(Serialize, Deserialize, TypewayCodec, Default)]
//! struct CreateUser {
//!     #[proto(tag = 1)]
//!     name: String,
//! }
//!
//! #[derive(Serialize, Deserialize, TypewayCodec, Default)]
//! struct User {
//!     #[proto(tag = 1)]
//!     id: u32,
//!     #[proto(tag = 2)]
//!     name: String,
//! }
//!
//! // Works for REST (JSON), gRPC+JSON, AND gRPC binary
//! async fn create_user(body: Proto<CreateUser>) -> Proto<User> {
//!     Proto::json(User { id: 3, name: body.name.clone() })
//! }
//! ```

use bytes::Bytes;
use http::request::Parts;
use http::StatusCode;

use typeway_protobuf::ProtoMessage;

use crate::body::{body_from_bytes, body_from_string, BoxBody};
use crate::extract::FromRequest;
use crate::response::IntoResponse;

/// The wire format detected during extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WireFormat {
    Json,
    BinaryProtobuf,
}

/// Format-agnostic protobuf extractor and response type.
///
/// Extracts `T` from either JSON or binary protobuf based on the request's
/// `Content-Type` header. Responds in the same format: binary requests get
/// binary responses (via `TypewayEncode`), JSON requests get JSON responses.
///
/// `T` must implement [`ProtoMessage`], which requires:
/// - `serde::Serialize + serde::Deserialize` (for JSON path)
/// - `TypewayEncode + TypewayDecode` (for binary protobuf path)
///
/// Use `#[derive(Serialize, Deserialize, TypewayCodec)]` to get all four.
pub struct Proto<T> {
    /// The inner value.
    inner: T,
    /// The wire format to use for the response.
    format: WireFormat,
}

impl<T> Proto<T> {
    /// Create a `Proto<T>` that will respond with JSON.
    pub fn json(value: T) -> Self {
        Proto { inner: value, format: WireFormat::Json }
    }

    /// Create a `Proto<T>` that will respond with binary protobuf.
    pub fn binary(value: T) -> Self {
        Proto { inner: value, format: WireFormat::BinaryProtobuf }
    }

    /// Get a reference to the inner value.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> std::ops::Deref for Proto<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> std::ops::DerefMut for Proto<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Proto<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Proto").field(&self.inner).finish()
    }
}

impl<T: Clone> Clone for Proto<T> {
    fn clone(&self) -> Self {
        Proto { inner: self.inner.clone(), format: self.format }
    }
}

/// Returns `true` if the content-type indicates binary protobuf encoding.
fn is_binary_protobuf(content_type: &str) -> bool {
    content_type == "application/grpc"
        || content_type == "application/grpc+proto"
        || content_type == "application/protobuf"
        || content_type == "application/x-protobuf"
}

impl<T: ProtoMessage + 'static> FromRequest for Proto<T> {
    type Error = (StatusCode, String);

    async fn from_request(parts: &Parts, body: Bytes) -> Result<Self, Self::Error> {
        let content_type = parts
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/json");

        if is_binary_protobuf(content_type) {
            // Fast path: binary protobuf → TypewayDecode (no JSON intermediate).
            // Uses typeway_decode_bytes for zero-copy string fields (BytesStr).
            T::typeway_decode_bytes(body)
                .map(|v| Proto { inner: v, format: WireFormat::BinaryProtobuf })
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("protobuf decode error: {e}")))
        } else {
            // JSON path: same as Json<T>
            serde_json::from_slice(&body)
                .map(|v| Proto { inner: v, format: WireFormat::Json })
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid JSON: {e}")))
        }
    }
}

impl<T: ProtoMessage> IntoResponse for Proto<T> {
    fn into_response(self) -> http::Response<BoxBody> {
        match self.format {
            WireFormat::BinaryProtobuf => {
                // Fast path: encode directly via TypewayEncode — no JSON.
                let encoded = self.inner.encode_to_vec();
                let body = body_from_bytes(Bytes::from(encoded));
                let mut res = http::Response::new(body);
                res.headers_mut().insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/protobuf"),
                );
                res
            }
            WireFormat::Json => {
                // JSON path: same as Json<T>.
                match serde_json::to_vec(&self.inner) {
                    Ok(bytes) => {
                        let body = body_from_bytes(Bytes::from(bytes));
                        let mut res = http::Response::new(body);
                        res.headers_mut().insert(
                            http::header::CONTENT_TYPE,
                            http::HeaderValue::from_static("application/json"),
                        );
                        res
                    }
                    Err(e) => {
                        let body = body_from_string(format!("serialization error: {e}"));
                        let mut res = http::Response::new(body);
                        *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                        res
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typeway_grpc::{TypewayDecode, TypewayDecodeError, TypewayEncode};

    // Manual impls for testing (in real usage, #[derive(TypewayCodec)] generates these)
    #[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestUser {
        id: u32,
        name: String,
    }

    impl TypewayEncode for TestUser {
        fn encoded_len(&self) -> usize {
            let mut len = 0;
            if self.id != 0 {
                len += 1 + typeway_grpc::tw_varint_len(self.id as u64);
            }
            if !self.name.is_empty() {
                len += 1 + typeway_grpc::tw_varint_len(self.name.len() as u64) + self.name.len();
            }
            len
        }

        fn encode_to(&self, buf: &mut Vec<u8>) {
            if self.id != 0 {
                typeway_grpc::tw_encode_tag(buf, 1, 0);
                typeway_grpc::tw_encode_varint(buf, self.id as u64);
            }
            if !self.name.is_empty() {
                typeway_grpc::tw_encode_tag(buf, 2, 2);
                typeway_grpc::tw_encode_varint(buf, self.name.len() as u64);
                buf.extend_from_slice(self.name.as_bytes());
            }
        }
    }

    impl TypewayDecode for TestUser {
        fn typeway_decode(bytes: &[u8]) -> Result<Self, TypewayDecodeError> {
            let mut user = TestUser::default();
            let mut offset = 0;
            while offset < bytes.len() {
                let (tag_wire, consumed) = typeway_grpc::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                let field_number = (tag_wire >> 3) as u32;
                let wire_type = (tag_wire & 0x07) as u8;
                match field_number {
                    1 => {
                        let (val, consumed) = typeway_grpc::tw_decode_varint(&bytes[offset..])?;
                        offset += consumed;
                        user.id = val as u32;
                    }
                    2 => {
                        let (len, consumed) = typeway_grpc::tw_decode_varint(&bytes[offset..])?;
                        offset += consumed;
                        let len = len as usize;
                        user.name = String::from_utf8(bytes[offset..offset + len].to_vec())
                            .map_err(|_| TypewayDecodeError::InvalidUtf8("name"))?;
                        offset += len;
                    }
                    _ => {
                        offset += typeway_grpc::tw_skip_wire_value(&bytes[offset..], wire_type)?;
                    }
                }
            }
            Ok(user)
        }
    }

    fn make_parts(content_type: &str) -> Parts {
        let (parts, _) = http::Request::builder()
            .header("content-type", content_type)
            .body(())
            .unwrap()
            .into_parts();
        parts
    }

    #[tokio::test]
    async fn proto_from_json() {
        let parts = make_parts("application/json");
        let body = Bytes::from(r#"{"id":42,"name":"Alice"}"#);

        let proto = Proto::<TestUser>::from_request(&parts, body).await.unwrap();
        assert_eq!(proto.id, 42);
        assert_eq!(proto.name, "Alice");
    }

    #[tokio::test]
    async fn proto_from_grpc_json() {
        let parts = make_parts("application/grpc+json");
        let body = Bytes::from(r#"{"id":1,"name":"Bob"}"#);

        let proto = Proto::<TestUser>::from_request(&parts, body).await.unwrap();
        assert_eq!(proto.id, 1);
        assert_eq!(proto.name, "Bob");
    }

    #[tokio::test]
    async fn proto_from_binary_protobuf() {
        let user = TestUser { id: 42, name: "Alice".into() };
        let binary = user.encode_to_vec();

        let parts = make_parts("application/grpc+proto");
        let body = Bytes::from(binary);

        let proto = Proto::<TestUser>::from_request(&parts, body).await.unwrap();
        assert_eq!(proto.id, 42);
        assert_eq!(proto.name, "Alice");
    }

    #[tokio::test]
    async fn proto_from_application_grpc() {
        let user = TestUser { id: 7, name: "Charlie".into() };
        let binary = user.encode_to_vec();

        let parts = make_parts("application/grpc");
        let body = Bytes::from(binary);

        let proto = Proto::<TestUser>::from_request(&parts, body).await.unwrap();
        assert_eq!(proto.id, 7);
        assert_eq!(proto.name, "Charlie");
    }

    #[tokio::test]
    async fn proto_from_application_protobuf() {
        let user = TestUser { id: 99, name: "Dave".into() };
        let binary = user.encode_to_vec();

        let parts = make_parts("application/protobuf");
        let body = Bytes::from(binary);

        let proto = Proto::<TestUser>::from_request(&parts, body).await.unwrap();
        assert_eq!(proto.id, 99);
    }

    #[tokio::test]
    async fn proto_invalid_json() {
        let parts = make_parts("application/json");
        let body = Bytes::from("not json");

        let result = Proto::<TestUser>::from_request(&parts, body).await;
        assert!(result.is_err());
        let (status, msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(msg.contains("invalid JSON"));
    }

    #[tokio::test]
    async fn proto_invalid_binary() {
        let parts = make_parts("application/grpc+proto");
        let body = Bytes::from_static(&[0xFF, 0xFF, 0xFF]);

        let result = Proto::<TestUser>::from_request(&parts, body).await;
        assert!(result.is_err());
        let (status, msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(msg.contains("protobuf decode error"));
    }

    #[test]
    fn proto_into_response_json() {
        let user = TestUser { id: 42, name: "Alice".into() };
        let response = Proto::json(user).into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );
    }

    #[test]
    fn proto_into_response_binary() {
        let user = TestUser { id: 42, name: "Alice".into() };
        let response = Proto::binary(user).into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/protobuf"
        );
    }

    #[tokio::test]
    async fn proto_binary_roundtrip_via_extractor() {
        // Encode a user as binary protobuf.
        let user = TestUser { id: 42, name: "Alice".into() };
        let binary = user.encode_to_vec();

        // Extract via Proto<T> with binary content-type.
        let parts = make_parts("application/grpc+proto");
        let proto = Proto::<TestUser>::from_request(&parts, Bytes::from(binary))
            .await
            .unwrap();
        assert_eq!(proto.id, 42);
        assert_eq!(proto.name, "Alice");

        // The response should be binary (format propagated from request).
        let response = proto.into_response();
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/protobuf"
        );

        // Decode the response body back.
        let body_bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        let decoded = TestUser::typeway_decode(&body_bytes).unwrap();
        assert_eq!(decoded.id, 42);
        assert_eq!(decoded.name, "Alice");
    }

    #[test]
    fn proto_debug_and_clone() {
        let p = Proto::json(TestUser { id: 1, name: "test".into() });
        let p2 = p.clone();
        assert_eq!(format!("{p:?}"), format!("{p2:?}"));
    }

    #[test]
    fn proto_deref() {
        let p = Proto::json(TestUser { id: 1, name: "test".into() });
        assert_eq!(p.id, 1);
        assert_eq!(p.name, "test");
    }
}
