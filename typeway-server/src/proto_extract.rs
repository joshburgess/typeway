//! The [`Proto<T>`] extractor — format-agnostic protobuf body extraction.
//!
//! `Proto<T>` works like `Json<T>` but auto-detects the wire format:
//! - `application/json` or `application/grpc+json` → serde JSON
//! - `application/grpc` or `application/protobuf` → `TypewayDecode` binary
//!
//! The same handler serves REST, gRPC+JSON, and gRPC binary clients.
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
//! // Works for REST (JSON), gRPC+JSON, AND gRPC binary — one handler
//! async fn create_user(Proto(req): Proto<CreateUser>) -> Proto<User> {
//!     Proto(User { id: 3, name: req.name })
//! }
//! ```

use bytes::Bytes;
use http::request::Parts;
use http::StatusCode;

use typeway_protobuf::ProtoMessage;

use crate::body::{body_from_bytes, body_from_string, BoxBody};
use crate::extract::FromRequest;
use crate::response::IntoResponse;

/// Format-agnostic protobuf extractor and response type.
///
/// Use it exactly like `Json<T>`:
///
/// ```ignore
/// async fn handler(Proto(req): Proto<CreateUser>) -> Proto<User> {
///     Proto(User { id: 1, name: req.name })
/// }
/// ```
///
/// On extraction, `Proto<T>` detects the content type and picks the
/// fastest deserializer. On response, it serializes as JSON (which the
/// gRPC dispatch transcodes to binary for binary clients automatically).
///
/// `T` must implement [`ProtoMessage`] — use
/// `#[derive(Serialize, Deserialize, TypewayCodec)]` to get it for free.
pub struct Proto<T>(pub T);

impl<T: std::fmt::Debug> std::fmt::Debug for Proto<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Proto").field(&self.0).finish()
    }
}

impl<T: Clone> Clone for Proto<T> {
    fn clone(&self) -> Self {
        Proto(self.0.clone())
    }
}

impl<T> std::ops::Deref for Proto<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Proto<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
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
            T::typeway_decode_bytes(body)
                .map(Proto)
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("protobuf decode error: {e}")))
        } else {
            // JSON path: same as Json<T>.
            serde_json::from_slice(&body)
                .map(Proto)
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid JSON: {e}")))
        }
    }
}

impl<T: ProtoMessage> IntoResponse for Proto<T> {
    fn into_response(self) -> http::Response<BoxBody> {
        // Respond as JSON. The gRPC dispatch handles binary transcoding
        // for binary clients automatically via wrap_response_as_grpc.
        match serde_json::to_vec(&self.0) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use typeway_protobuf::{TypewayDecode, TypewayDecodeError, TypewayEncode};

    #[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestUser {
        id: u32,
        name: String,
    }

    impl TypewayEncode for TestUser {
        fn encoded_len(&self) -> usize {
            let mut len = 0;
            if self.id != 0 {
                len += 1 + typeway_protobuf::tw_varint_len(self.id as u64);
            }
            if !self.name.is_empty() {
                len += 1 + typeway_protobuf::tw_varint_len(self.name.len() as u64) + self.name.len();
            }
            len
        }

        fn encode_to(&self, buf: &mut Vec<u8>) {
            if self.id != 0 {
                typeway_protobuf::tw_encode_tag(buf, 1, 0);
                typeway_protobuf::tw_encode_varint(buf, self.id as u64);
            }
            if !self.name.is_empty() {
                typeway_protobuf::tw_encode_tag(buf, 2, 2);
                typeway_protobuf::tw_encode_varint(buf, self.name.len() as u64);
                buf.extend_from_slice(self.name.as_bytes());
            }
        }
    }

    impl TypewayDecode for TestUser {
        fn typeway_decode(bytes: &[u8]) -> Result<Self, TypewayDecodeError> {
            let mut user = TestUser::default();
            let mut offset = 0;
            while offset < bytes.len() {
                let (tag_wire, consumed) = typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                let field_number = (tag_wire >> 3) as u32;
                let wire_type = (tag_wire & 0x07) as u8;
                match field_number {
                    1 => {
                        let (val, consumed) = typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                        offset += consumed;
                        user.id = val as u32;
                    }
                    2 => {
                        let (len, consumed) = typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                        offset += consumed;
                        let len = len as usize;
                        user.name = String::from_utf8(bytes[offset..offset + len].to_vec())
                            .map_err(|_| TypewayDecodeError::InvalidUtf8("name"))?;
                        offset += len;
                    }
                    _ => {
                        offset += typeway_protobuf::tw_skip_wire_value(&bytes[offset..], wire_type)?;
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
        let Proto(user) = Proto::<TestUser>::from_request(&parts, body).await.unwrap();
        assert_eq!(user.id, 42);
        assert_eq!(user.name, "Alice");
    }

    #[tokio::test]
    async fn proto_from_grpc_json() {
        let parts = make_parts("application/grpc+json");
        let body = Bytes::from(r#"{"id":1,"name":"Bob"}"#);
        let Proto(user) = Proto::<TestUser>::from_request(&parts, body).await.unwrap();
        assert_eq!(user.id, 1);
    }

    #[tokio::test]
    async fn proto_from_binary() {
        let user = TestUser { id: 42, name: "Alice".into() };
        let binary = user.encode_to_vec();
        let parts = make_parts("application/grpc+proto");
        let Proto(decoded) = Proto::<TestUser>::from_request(&parts, Bytes::from(binary)).await.unwrap();
        assert_eq!(decoded.id, 42);
        assert_eq!(decoded.name, "Alice");
    }

    #[tokio::test]
    async fn proto_from_application_grpc() {
        let user = TestUser { id: 7, name: "Charlie".into() };
        let binary = user.encode_to_vec();
        let parts = make_parts("application/grpc");
        let Proto(decoded) = Proto::<TestUser>::from_request(&parts, Bytes::from(binary)).await.unwrap();
        assert_eq!(decoded.id, 7);
    }

    #[tokio::test]
    async fn proto_from_application_protobuf() {
        let user = TestUser { id: 99, name: "Dave".into() };
        let binary = user.encode_to_vec();
        let parts = make_parts("application/protobuf");
        let Proto(decoded) = Proto::<TestUser>::from_request(&parts, Bytes::from(binary)).await.unwrap();
        assert_eq!(decoded.id, 99);
    }

    #[tokio::test]
    async fn proto_invalid_json() {
        let parts = make_parts("application/json");
        let result = Proto::<TestUser>::from_request(&parts, Bytes::from("not json")).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn proto_invalid_binary() {
        let parts = make_parts("application/grpc+proto");
        let result = Proto::<TestUser>::from_request(&parts, Bytes::from_static(&[0xFF, 0xFF])).await;
        assert!(result.is_err());
    }

    #[test]
    fn proto_into_response_json() {
        let response = Proto(TestUser { id: 42, name: "Alice".into() }).into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get("content-type").unwrap(), "application/json");
    }

    #[test]
    fn proto_tuple_destructure() {
        let Proto(user) = Proto(TestUser { id: 1, name: "test".into() });
        assert_eq!(user.id, 1);
    }

    #[test]
    fn proto_deref() {
        let p = Proto(TestUser { id: 1, name: "test".into() });
        assert_eq!(p.id, 1);
        assert_eq!(p.name, "test");
    }

    #[test]
    fn proto_debug_and_clone() {
        let p = Proto(TestUser { id: 1, name: "test".into() });
        let p2 = p.clone();
        assert_eq!(format!("{p:?}"), format!("{p2:?}"));
    }
}
