//! `typeway-protobuf` — type-theoretic protobuf layer for the Typeway framework.
//!
//! This crate provides the [`ProtoMessage`] trait and [`BytesStr`] type that
//! enable zero-JSON-intermediate protobuf handling. Types that implement
//! `ProtoMessage` can be deserialized from either JSON or binary protobuf
//! by the [`Proto<T>`](https://docs.rs/typeway-server) extractor.
//!
//! # ProtoMessage
//!
//! A convenience trait combining the four bounds needed for format-agnostic
//! message handling:
//!
//! ```ignore
//! #[derive(Serialize, Deserialize, TypewayCodec, Default)]
//! struct User {
//!     #[proto(tag = 1)]
//!     id: u32,
//!     #[proto(tag = 2)]
//!     name: String,
//! }
//! // User automatically implements ProtoMessage
//! ```
//!
//! # BytesStr
//!
//! A zero-copy string type backed by `bytes::Bytes`. Validates UTF-8 on
//! construction and provides `Deref<Target = str>` for ergonomic use.

use bytes::Bytes;

// Re-export the codec traits for convenience.
pub use typeway_grpc::{TypewayDecode, TypewayDecodeError, TypewayEncode};

/// A protobuf message type that supports both JSON and binary encoding.
///
/// This is automatically implemented for any type that derives both
/// `serde::Serialize + serde::Deserialize` and `TypewayCodec`.
///
/// Used as the bound for the `Proto<T>` extractor in typeway-server.
pub trait ProtoMessage:
    TypewayEncode + TypewayDecode + serde::Serialize + serde::de::DeserializeOwned + Send + Sized
{
}

/// Blanket implementation — any type with all four derives gets ProtoMessage for free.
impl<T> ProtoMessage for T where
    T: TypewayEncode + TypewayDecode + serde::Serialize + serde::de::DeserializeOwned + Send + Sized
{
}

/// A zero-copy string backed by `bytes::Bytes`.
///
/// `BytesStr` validates UTF-8 on construction and provides `Deref<Target = str>`.
/// Cloning is cheap (reference count increment on the underlying `Bytes`).
///
/// This is the building block for zero-copy protobuf string fields in future
/// typeway-protobuf types. For the current `Proto<T>` extractor, standard
/// `String` fields work fine — `BytesStr` is for performance-critical paths
/// where avoiding allocation matters.
///
/// # Example
///
/// ```
/// use typeway_protobuf::BytesStr;
/// use bytes::Bytes;
///
/// let s = BytesStr::from_utf8(Bytes::from_static(b"hello")).unwrap();
/// assert_eq!(&*s, "hello");
/// assert_eq!(s.len(), 5);
///
/// // Cloning is cheap (refcount increment)
/// let s2 = s.clone();
/// assert_eq!(&*s2, "hello");
/// ```
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct BytesStr {
    inner: Bytes,
}

impl BytesStr {
    /// Create a `BytesStr` from `Bytes`, validating UTF-8.
    pub fn from_utf8(bytes: Bytes) -> Result<Self, std::str::Utf8Error> {
        std::str::from_utf8(&bytes)?;
        Ok(BytesStr { inner: bytes })
    }

    /// Create a `BytesStr` from `Bytes` without UTF-8 validation.
    ///
    /// # Safety
    ///
    /// The caller must guarantee the bytes are valid UTF-8.
    pub unsafe fn from_utf8_unchecked(bytes: Bytes) -> Self {
        BytesStr { inner: bytes }
    }

    /// Return the string as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    /// Return the underlying `Bytes`.
    pub fn into_bytes(self) -> Bytes {
        self.inner
    }

    /// Return the string length in bytes.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Return whether the string is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl std::ops::Deref for BytesStr {
    type Target = str;

    fn deref(&self) -> &str {
        // Safety: validated at construction
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
        BytesStr {
            inner: Bytes::from(s),
        }
    }
}

impl From<&'static str> for BytesStr {
    fn from(s: &'static str) -> Self {
        BytesStr {
            inner: Bytes::from_static(s.as_bytes()),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_str_from_utf8() {
        let s = BytesStr::from_utf8(Bytes::from_static(b"hello")).unwrap();
        assert_eq!(&*s, "hello");
        assert_eq!(s.len(), 5);
        assert!(!s.is_empty());
    }

    #[test]
    fn bytes_str_invalid_utf8() {
        let result = BytesStr::from_utf8(Bytes::from_static(&[0xFF, 0xFE]));
        assert!(result.is_err());
    }

    #[test]
    fn bytes_str_clone_is_cheap() {
        let s = BytesStr::from("hello world".to_string());
        let s2 = s.clone();
        assert_eq!(&*s, &*s2);
    }

    #[test]
    fn bytes_str_display() {
        let s = BytesStr::from("test");
        assert_eq!(format!("{s}"), "test");
    }

    #[test]
    fn bytes_str_debug() {
        let s = BytesStr::from("test");
        assert_eq!(format!("{s:?}"), "\"test\"");
    }

    #[test]
    fn bytes_str_serde_roundtrip() {
        let original = BytesStr::from("hello");
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"hello\"");
        let deserialized: BytesStr = serde_json::from_str(&json).unwrap();
        assert_eq!(&*deserialized, "hello");
    }

    #[test]
    fn bytes_str_empty() {
        let s = BytesStr::from_utf8(Bytes::new()).unwrap();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

}
