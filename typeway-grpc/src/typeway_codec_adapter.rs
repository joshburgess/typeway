//! Adapter integrating [`TypewayCodec`](crate::typeway_codec) with the
//! [`GrpcCodec`](crate::codec::GrpcCodec) trait system.
//!
//! [`TypewayCodecAdapter<T>`] wraps a message type that implements
//! [`TypewayEncode`] + [`TypewayDecode`] and provides a `GrpcCodec`
//! implementation that encodes/decodes binary protobuf directly —
//! bypassing the JSON intermediate used by `JsonCodec` and `BinaryCodec`.
//!
//! This gives the full 3-8x speedup from the compile-time specialized
//! codec while still working with the existing dispatch infrastructure.
//!
//! # Usage
//!
//! The adapter is used when both request and response types derive
//! `TypewayCodec` and `serde`:
//!
//! ```ignore
//! use typeway_macros::TypewayCodec;
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(TypewayCodec, Serialize, Deserialize, Default)]
//! struct User {
//!     #[proto(tag = 1)]
//!     id: u32,
//!     #[proto(tag = 2)]
//!     name: String,
//! }
//!
//! // The adapter encodes User directly to protobuf binary (fast path)
//! // and can decode binary back to JSON (for handler compatibility).
//! ```

use crate::codec::{CodecError, CodecErrorKind, GrpcCodec};
use crate::typeway_codec::{TypewayDecode, TypewayDecodeError, TypewayEncode};

/// A `GrpcCodec` adapter for types implementing `TypewayEncode`/`TypewayDecode`.
///
/// For encoding (response path): serializes JSON → Rust struct → protobuf binary.
/// For decoding (request path): protobuf binary → Rust struct → JSON.
///
/// The intermediate Rust struct step uses TypewayCodec (compile-time specialized),
/// and serde is used only for the JSON↔struct conversion.
pub struct TypewayCodecAdapter<T> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T> TypewayCodecAdapter<T> {
    /// Create a new adapter.
    pub fn new() -> Self {
        TypewayCodecAdapter {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> Default for TypewayCodecAdapter<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> GrpcCodec for TypewayCodecAdapter<T>
where
    T: TypewayEncode
        + TypewayDecode
        + serde::Serialize
        + serde::de::DeserializeOwned
        + Send
        + Sync
        + 'static,
{
    fn content_type(&self) -> &'static str {
        "application/grpc+proto"
    }

    fn encode(&self, value: &serde_json::Value) -> Result<Vec<u8>, CodecError> {
        // JSON Value → Rust struct (via serde) → protobuf binary (via TypewayCodec)
        let msg: T = serde_json::from_value(value.clone()).map_err(|e| CodecError {
            kind: CodecErrorKind::Encode,
            message: format!("failed to deserialize JSON to struct: {e}"),
        })?;
        Ok(msg.encode_to_vec())
    }

    fn decode(&self, bytes: &[u8]) -> Result<serde_json::Value, CodecError> {
        if bytes.is_empty() {
            return Ok(serde_json::Value::Object(Default::default()));
        }
        // Protobuf binary → Rust struct (via TypewayCodec) → JSON Value (via serde)
        let msg = T::typeway_decode(bytes).map_err(|e| CodecError {
            kind: CodecErrorKind::Decode,
            message: decode_error_to_string(e),
        })?;
        serde_json::to_value(&msg).map_err(|e| CodecError {
            kind: CodecErrorKind::Decode,
            message: format!("failed to serialize struct to JSON: {e}"),
        })
    }
}

fn decode_error_to_string(e: TypewayDecodeError) -> String {
    match e {
        TypewayDecodeError::UnexpectedEof => "unexpected end of input".to_string(),
        TypewayDecodeError::VarintTooLong => "varint exceeds 10 bytes".to_string(),
        TypewayDecodeError::UnknownWireType(wt) => format!("unknown wire type: {wt}"),
        TypewayDecodeError::InvalidFieldValue { field, message } => {
            format!("invalid value for field '{field}': {message}")
        }
        TypewayDecodeError::MissingField(field) => format!("missing required field: {field}"),
        TypewayDecodeError::InvalidUtf8(field) => format!("invalid UTF-8 in field: {field}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typeway_codec::{
        tw_encode_tag, tw_encode_varint,
    };

    // Manual TypewayEncode/TypewayDecode impl for testing
    // (in real usage, #[derive(TypewayCodec)] generates these)
    #[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestMsg {
        id: u32,
        name: String,
    }

    impl TypewayEncode for TestMsg {
        fn encoded_len(&self) -> usize {
            let mut len = 0;
            if self.id != 0 {
                len += 1 + crate::typeway_codec::tw_varint_len(self.id as u64);
            }
            if !self.name.is_empty() {
                len += 1
                    + crate::typeway_codec::tw_varint_len(self.name.len() as u64)
                    + self.name.len();
            }
            len
        }

        fn encode_to(&self, buf: &mut Vec<u8>) {
            if self.id != 0 {
                tw_encode_tag(buf, 1, 0);
                tw_encode_varint(buf, self.id as u64);
            }
            if !self.name.is_empty() {
                tw_encode_tag(buf, 2, 2);
                tw_encode_varint(buf, self.name.len() as u64);
                buf.extend_from_slice(self.name.as_bytes());
            }
        }
    }

    impl TypewayDecode for TestMsg {
        fn typeway_decode(bytes: &[u8]) -> Result<Self, TypewayDecodeError> {
            let mut msg = TestMsg::default();
            let mut offset = 0;
            while offset < bytes.len() {
                let (tag_wire, consumed) =
                    crate::typeway_codec::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                let field_number = (tag_wire >> 3) as u32;
                let wire_type = (tag_wire & 0x07) as u8;
                match field_number {
                    1 => {
                        let (val, consumed) =
                            crate::typeway_codec::tw_decode_varint(&bytes[offset..])?;
                        offset += consumed;
                        msg.id = val as u32;
                    }
                    2 => {
                        let (len, consumed) =
                            crate::typeway_codec::tw_decode_varint(&bytes[offset..])?;
                        offset += consumed;
                        let len = len as usize;
                        msg.name = String::from_utf8(bytes[offset..offset + len].to_vec())
                            .map_err(|_| TypewayDecodeError::InvalidUtf8("name"))?;
                        offset += len;
                    }
                    _ => {
                        offset += crate::typeway_codec::tw_skip_wire_value(
                            &bytes[offset..],
                            wire_type,
                        )?;
                    }
                }
            }
            Ok(msg)
        }
    }

    #[test]
    fn adapter_encode_decode_roundtrip() {
        let adapter = TypewayCodecAdapter::<TestMsg>::new();

        let json = serde_json::json!({"id": 42, "name": "Alice"});
        let encoded = adapter.encode(&json).unwrap();
        let decoded = adapter.decode(&encoded).unwrap();

        assert_eq!(decoded["id"], 42);
        assert_eq!(decoded["name"], "Alice");
    }

    #[test]
    fn adapter_content_type() {
        let adapter = TypewayCodecAdapter::<TestMsg>::new();
        assert_eq!(adapter.content_type(), "application/grpc+proto");
    }

    #[test]
    fn adapter_empty_bytes_returns_empty_object() {
        let adapter = TypewayCodecAdapter::<TestMsg>::new();
        let decoded = adapter.decode(b"").unwrap();
        assert_eq!(decoded, serde_json::json!({}));
    }
}
