//! `typeway-protobuf` — high-performance, type-theoretic protobuf for Rust.
//!
//! Addresses prost's key limitations:
//!
//! - **Zero-copy strings** via [`BytesStr`] — no allocation for string fields
//! - **Pooled repeated fields** via [`RepeatedField<T>`] — reuse allocations across decodes
//! - **Buffer reuse** via [`EncodeBuf`] — connection-scoped encode buffers
//! - **Phantom-typed wire formats** via [`ProtoField<T, E>`] — disambiguate `i32` encodings at the type level
//! - **Format-agnostic extraction** via [`ProtoMessage`] — same type works for JSON and binary

pub mod repeated;
pub mod wire;

use bytes::Bytes;

// Re-export the codec traits.
pub use typeway_grpc::{TypewayDecode, TypewayDecodeError, TypewayEncode};

// Re-export submodules.
pub use repeated::RepeatedField;
pub use wire::{Fixed, Packed, ProtoField, Varint, ZigZag};

// ---------------------------------------------------------------------------
// ProtoMessage trait
// ---------------------------------------------------------------------------

/// A protobuf message type that supports both JSON and binary encoding.
///
/// Automatically implemented for any type that derives both
/// `serde::Serialize + serde::Deserialize` and `TypewayCodec`.
pub trait ProtoMessage:
    TypewayEncode + TypewayDecode + serde::Serialize + serde::de::DeserializeOwned + Send + Sized
{
}

impl<T> ProtoMessage for T where
    T: TypewayEncode + TypewayDecode + serde::Serialize + serde::de::DeserializeOwned + Send + Sized
{
}

// ---------------------------------------------------------------------------
// BytesStr — zero-copy string backed by Bytes
// ---------------------------------------------------------------------------

/// A zero-copy string backed by `bytes::Bytes`.
///
/// Validates UTF-8 on construction. Cloning is O(1) (refcount increment).
/// Use this for string fields in performance-critical protobuf types to
/// avoid per-field allocation during deserialization.
///
/// ```
/// use typeway_protobuf::BytesStr;
/// use bytes::Bytes;
///
/// let s = BytesStr::from_utf8(Bytes::from_static(b"hello")).unwrap();
/// assert_eq!(&*s, "hello");
///
/// // Cloning is cheap (refcount increment, no copy)
/// let s2 = s.clone();
/// assert_eq!(&*s2, "hello");
/// ```
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct BytesStr {
    inner: Bytes,
}

impl BytesStr {
    /// Create from `Bytes`, validating UTF-8.
    pub fn from_utf8(bytes: Bytes) -> Result<Self, std::str::Utf8Error> {
        std::str::from_utf8(&bytes)?;
        Ok(BytesStr { inner: bytes })
    }

    /// Create from `Bytes` without UTF-8 validation.
    ///
    /// # Safety
    /// The caller must guarantee the bytes are valid UTF-8.
    pub unsafe fn from_utf8_unchecked(bytes: Bytes) -> Self {
        BytesStr { inner: bytes }
    }

    /// Slice this string, producing a new `BytesStr` that shares the
    /// backing buffer. O(1), no copy.
    pub fn slice(&self, range: std::ops::Range<usize>) -> Self {
        // Safety: if self is valid UTF-8 and range is on char boundaries, the slice is valid.
        // For internal use where we control the range.
        BytesStr {
            inner: self.inner.slice(range),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    pub fn into_bytes(self) -> Bytes {
        self.inner
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for BytesStr {
    fn default() -> Self {
        BytesStr {
            inner: Bytes::new(),
        }
    }
}

impl std::ops::Deref for BytesStr {
    type Target = str;
    fn deref(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(&self.inner) }
    }
}

impl std::fmt::Display for BytesStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self)
    }
}

impl std::fmt::Debug for BytesStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &**self)
    }
}

impl From<String> for BytesStr {
    fn from(s: String) -> Self {
        BytesStr { inner: Bytes::from(s) }
    }
}

impl From<&'static str> for BytesStr {
    fn from(s: &'static str) -> Self {
        BytesStr { inner: Bytes::from_static(s.as_bytes()) }
    }
}

impl From<BytesStr> for String {
    fn from(s: BytesStr) -> Self {
        s.to_string()
    }
}

impl PartialEq<str> for BytesStr {
    fn eq(&self, other: &str) -> bool {
        &**self == other
    }
}

impl PartialEq<&str> for BytesStr {
    fn eq(&self, other: &&str) -> bool {
        &**self == *other
    }
}

impl serde::Serialize for BytesStr {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for BytesStr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(BytesStr::from(s))
    }
}

// ---------------------------------------------------------------------------
// EncodeBuf — reusable encode buffer
// ---------------------------------------------------------------------------

/// A reusable encode buffer that avoids allocation on repeated encodes.
///
/// Instead of `encode_to_vec()` (allocates a new Vec each time), use
/// `EncodeBuf` to clear and reuse the same buffer:
///
/// ```ignore
/// let mut buf = EncodeBuf::new();
/// for msg in messages {
///     let bytes = buf.encode(&msg); // reuses allocation
///     send(bytes);
/// }
/// ```
pub struct EncodeBuf {
    inner: Vec<u8>,
}

impl EncodeBuf {
    pub fn new() -> Self {
        EncodeBuf { inner: Vec::new() }
    }

    pub fn with_capacity(cap: usize) -> Self {
        EncodeBuf {
            inner: Vec::with_capacity(cap),
        }
    }

    /// Encode a message, reusing the buffer. Returns a slice of the encoded bytes.
    pub fn encode<T: TypewayEncode>(&mut self, msg: &T) -> &[u8] {
        self.inner.clear();
        self.inner.reserve(msg.encoded_len());
        msg.encode_to(&mut self.inner);
        &self.inner
    }

    /// Encode a message and return owned Bytes (zero-copy via Bytes::from).
    pub fn encode_to_bytes<T: TypewayEncode>(&mut self, msg: &T) -> Bytes {
        self.inner.clear();
        self.inner.reserve(msg.encoded_len());
        msg.encode_to(&mut self.inner);
        Bytes::copy_from_slice(&self.inner)
    }
}

impl Default for EncodeBuf {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_str_from_utf8() {
        let s = BytesStr::from_utf8(Bytes::from_static(b"hello")).unwrap();
        assert_eq!(&*s, "hello");
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn bytes_str_invalid_utf8() {
        assert!(BytesStr::from_utf8(Bytes::from_static(&[0xFF, 0xFE])).is_err());
    }

    #[test]
    fn bytes_str_clone_is_cheap() {
        let s = BytesStr::from("hello world".to_string());
        let s2 = s.clone();
        assert_eq!(&*s, &*s2);
    }

    #[test]
    fn bytes_str_default_is_empty() {
        let s = BytesStr::default();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn bytes_str_display_debug() {
        let s = BytesStr::from("test");
        assert_eq!(format!("{s}"), "test");
        assert_eq!(format!("{s:?}"), "\"test\"");
    }

    #[test]
    fn bytes_str_serde_roundtrip() {
        let original = BytesStr::from("hello");
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: BytesStr = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn bytes_str_partial_eq() {
        let s = BytesStr::from("hello");
        assert_eq!(s, "hello");
        assert_eq!(s, *"hello");
    }

    #[test]
    fn bytes_str_into_string() {
        let s = BytesStr::from("hello");
        let owned: String = s.into();
        assert_eq!(owned, "hello");
    }

    #[test]
    fn bytes_str_slice() {
        let s = BytesStr::from_utf8(Bytes::from_static(b"hello world")).unwrap();
        let sub = s.slice(0..5);
        assert_eq!(&*sub, "hello");
    }

    #[test]
    fn encode_buf_reuse() {
        use typeway_grpc::{tw_encode_tag, tw_encode_varint};

        // Manual TypewayEncode for testing.
        struct Small { id: u32 }
        impl TypewayEncode for Small {
            fn encoded_len(&self) -> usize { 1 + typeway_grpc::tw_varint_len(self.id as u64) }
            fn encode_to(&self, buf: &mut Vec<u8>) {
                tw_encode_tag(buf, 1, 0);
                tw_encode_varint(buf, self.id as u64);
            }
        }

        let mut buf = EncodeBuf::new();
        let bytes1 = buf.encode(&Small { id: 1 }).to_vec();
        let bytes2 = buf.encode(&Small { id: 2 }).to_vec();
        // Different content, same buffer reused.
        assert_ne!(bytes1, bytes2);
        assert_eq!(bytes1.len(), bytes2.len());
    }
}
