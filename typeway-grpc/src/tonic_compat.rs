//! Tonic compatibility layer for using prost-generated types with typeway.
//!
//! When the `tonic-compat` feature is enabled, this module provides:
//!
//! - [`Protobuf<T>`] -- an extractor and response type for protobuf-encoded bodies
//! - `impl_proto_type_for_prost!` -- bridges prost `Message` types with typeway's
//!   [`ToProtoType`](crate::ToProtoType) trait
//! - [`prost_to_json`] and [`json_to_prost`] -- conversion helpers between prost
//!   messages and serde JSON values
//!
//! This avoids dual serialization: prost-generated types can be used directly in
//! typeway handlers without requiring serde derives.

/// Extractor and response type for Protocol Buffers encoded bodies.
///
/// Similar to `Json<T>` but uses protobuf binary encoding via `prost`.
/// The type `T` must implement [`prost::Message`] and [`Default`].
///
/// # As an extractor
///
/// ```ignore
/// use typeway_grpc::tonic_compat::Protobuf;
///
/// async fn create_user(body: Protobuf<CreateUserRequest>) -> Protobuf<UserResponse> {
///     let req = body.0;
///     // ... handle request ...
///     Protobuf(UserResponse { /* ... */ })
/// }
/// ```
///
/// # As a response
///
/// When returned from a handler, `Protobuf<T>` serializes the value
/// as protobuf binary with `content-type: application/protobuf`.
pub struct Protobuf<T>(pub T);

impl<T: prost::Message + Default> Protobuf<T> {
    /// Decode a protobuf message from bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtobufError> {
        T::decode(bytes)
            .map(Protobuf)
            .map_err(|e| ProtobufError::Decode(e.to_string()))
    }

    /// Encode the inner value to protobuf bytes.
    pub fn encode(&self) -> Vec<u8> {
        self.0.encode_to_vec()
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Protobuf<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Protobuf").field(&self.0).finish()
    }
}

impl<T: Clone> Clone for Protobuf<T> {
    fn clone(&self) -> Self {
        Protobuf(self.0.clone())
    }
}

/// Errors from protobuf encoding/decoding.
#[derive(Debug, Clone)]
pub enum ProtobufError {
    /// Failed to decode a protobuf message from bytes.
    Decode(String),
    /// Failed to encode a protobuf message to bytes.
    Encode(String),
}

impl std::fmt::Display for ProtobufError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "protobuf decode error: {e}"),
            Self::Encode(e) => write!(f, "protobuf encode error: {e}"),
        }
    }
}

impl std::error::Error for ProtobufError {}

// ---------------------------------------------------------------------------
// impl_proto_type_for_prost! macro
// ---------------------------------------------------------------------------

/// Implement [`ToProtoType`](crate::ToProtoType) for a prost-generated message type.
///
/// This bridges prost's `Message` trait with typeway's `ToProtoType` trait,
/// allowing prost-generated types to be used in API type definitions and
/// proto generation.
///
/// # Examples
///
/// Basic usage with the type name as the proto name:
///
/// ```ignore
/// // In your build.rs, prost generates:
/// // pub struct User { pub id: u32, pub name: String }
///
/// impl_proto_type_for_prost!(User);
/// // Now User implements ToProtoType with proto_type_name() == "User"
/// ```
///
/// With a custom fully-qualified proto name:
///
/// ```ignore
/// impl_proto_type_for_prost!(User, "users.v1.User");
/// // proto_type_name() == "users.v1.User"
/// ```
#[macro_export]
macro_rules! impl_proto_type_for_prost {
    ($type:ty) => {
        impl $crate::ToProtoType for $type {
            fn proto_type_name() -> &'static str {
                stringify!($type)
            }
            fn is_message() -> bool {
                true
            }
            fn message_definition() -> Option<String> {
                // prost types are defined in .proto files, so the definition
                // already exists there. Return None to indicate no generated
                // definition is needed.
                None
            }
        }
    };
    ($type:ty, $proto_name:expr) => {
        impl $crate::ToProtoType for $type {
            fn proto_type_name() -> &'static str {
                $proto_name
            }
            fn is_message() -> bool {
                true
            }
            fn message_definition() -> Option<String> {
                None
            }
        }
    };
}

// ---------------------------------------------------------------------------
// JSON <-> prost conversion helpers
// ---------------------------------------------------------------------------

/// Convert a prost `Message` to a [`serde_json::Value`].
///
/// Since prost does not have built-in JSON support, this encodes the message
/// as protobuf binary and wraps it in a JSON object with a
/// `_protobuf_binary` key containing the base64-encoded bytes.
///
/// For proper JSON mapping of protobuf messages, consider using
/// `prost-reflect` or implementing `serde::Serialize` on your types.
pub fn prost_to_json<T: prost::Message>(msg: &T) -> Result<serde_json::Value, ProtobufError> {
    let bytes = msg.encode_to_vec();
    Ok(serde_json::json!({
        "_protobuf_binary": base64_encode(&bytes)
    }))
}

/// Convert a [`serde_json::Value`] to a prost `Message`.
///
/// Expects the JSON to contain a `_protobuf_binary` key with base64-encoded
/// protobuf bytes (as produced by [`prost_to_json`]).
///
/// Returns an error if the JSON does not contain the expected key or if
/// decoding fails.
pub fn json_to_prost<T: prost::Message + Default>(
    json: &serde_json::Value,
) -> Result<T, ProtobufError> {
    if let Some(b64) = json.get("_protobuf_binary").and_then(|v| v.as_str()) {
        let bytes =
            base64_decode(b64).map_err(|e| ProtobufError::Decode(format!("base64 decode: {e}")))?;
        T::decode(bytes.as_slice()).map_err(|e| ProtobufError::Decode(e.to_string()))
    } else {
        Err(ProtobufError::Decode(
            "direct JSON to prost conversion not supported \
             -- use protobuf binary or provide a _protobuf_binary field"
                .to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Base64 helpers (self-contained, no external dependency)
// ---------------------------------------------------------------------------

/// Encode bytes as standard base64 (RFC 4648).
pub fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Decode a standard base64 (RFC 4648) string to bytes.
pub fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let input = input.trim_end_matches('=');
    let mut result = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for c in input.chars() {
        let val = match c {
            'A'..='Z' => (c as u32) - ('A' as u32),
            'a'..='z' => (c as u32) - ('a' as u32) + 26,
            '0'..='9' => (c as u32) - ('0' as u32) + 52,
            '+' => 62,
            '/' => 63,
            _ => return Err(format!("invalid base64 char: {c}")),
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::ToProtoType;

    #[test]
    fn base64_roundtrip() {
        let data = b"Hello, protobuf!";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64_roundtrip_empty() {
        let encoded = base64_encode(b"");
        let decoded = base64_decode(&encoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn base64_roundtrip_single_byte() {
        let data = b"A";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64_roundtrip_two_bytes() {
        let data = b"AB";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64_decode_invalid_char() {
        assert!(base64_decode("invalid!").is_err());
    }

    #[test]
    fn protobuf_error_display_decode() {
        let err = ProtobufError::Decode("bad data".into());
        assert!(err.to_string().contains("bad data"));
        assert!(err.to_string().contains("decode"));
    }

    #[test]
    fn protobuf_error_display_encode() {
        let err = ProtobufError::Encode("too large".into());
        assert!(err.to_string().contains("too large"));
        assert!(err.to_string().contains("encode"));
    }

    #[test]
    fn protobuf_error_is_error_trait() {
        let err: Box<dyn std::error::Error> = Box::new(ProtobufError::Decode("test".into()));
        assert!(err.to_string().contains("test"));
    }

    #[test]
    fn impl_proto_type_macro_compiles() {
        struct FakeMessage;
        impl_proto_type_for_prost!(FakeMessage);
        assert_eq!(FakeMessage::proto_type_name(), "FakeMessage");
        assert!(FakeMessage::is_message());
        assert!(FakeMessage::message_definition().is_none());
    }

    #[test]
    fn impl_proto_type_macro_with_custom_name() {
        struct FakeMsg;
        impl_proto_type_for_prost!(FakeMsg, "custom.FakeMsg");
        assert_eq!(FakeMsg::proto_type_name(), "custom.FakeMsg");
        assert!(FakeMsg::is_message());
        assert!(FakeMsg::message_definition().is_none());
    }

    #[test]
    fn protobuf_debug() {
        let p = Protobuf(42u32);
        let debug = format!("{p:?}");
        assert!(debug.contains("Protobuf"));
        assert!(debug.contains("42"));
    }

    #[test]
    fn protobuf_clone() {
        let p = Protobuf(String::from("hello"));
        let p2 = p.clone();
        assert_eq!(p.0, p2.0);
    }
}
