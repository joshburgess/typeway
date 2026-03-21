//! Phantom-typed wire format discrimination.
//!
//! Prost maps all protobuf enums to `i32` and collapses multiple wire
//! encodings (`int32`, `sint32`, `sfixed32`) into the same Rust type.
//! This makes trait-based dispatch impossible and loses type information.
//!
//! `ProtoField<T, Encoding>` uses zero-sized phantom types to distinguish
//! wire encodings at the type level. Six different wire representations
//! of `i32` become six distinct Rust types — with zero runtime cost.
//!
//! ```
//! use typeway_protobuf::wire::*;
//!
//! // These are all i32 at runtime, but distinct types at compile time:
//! type RegularInt = ProtoField<i32, Varint>;      // int32
//! type SignedInt  = ProtoField<i32, ZigZag>;      // sint32
//! type FixedInt   = ProtoField<i32, Fixed>;       // sfixed32
//! type PackedInts = ProtoField<Vec<i32>, Packed<Varint>>; // packed repeated int32
//! ```

use std::marker::PhantomData;

/// A protobuf field value with phantom-typed wire encoding.
///
/// The value is stored as `T` at runtime. The `E` parameter is a
/// zero-sized type that determines how the value is encoded/decoded
/// on the wire. This is erased at compile time — zero runtime cost.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProtoField<T, E> {
    /// The actual value.
    pub value: T,
    _encoding: PhantomData<E>,
}

impl<T, E> ProtoField<T, E> {
    /// Create a new `ProtoField` with the given value.
    pub fn new(value: T) -> Self {
        ProtoField {
            value,
            _encoding: PhantomData,
        }
    }

    /// Extract the inner value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T: Default, E> Default for ProtoField<T, E> {
    fn default() -> Self {
        ProtoField::new(T::default())
    }
}

impl<T: std::fmt::Debug, E> std::fmt::Debug for ProtoField<T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<T: std::fmt::Display, E> std::fmt::Display for ProtoField<T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<T, E> std::ops::Deref for ProtoField<T, E> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T, E> std::ops::DerefMut for ProtoField<T, E> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T, E> From<T> for ProtoField<T, E> {
    fn from(value: T) -> Self {
        ProtoField::new(value)
    }
}

impl<T: serde::Serialize, E> serde::Serialize for ProtoField<T, E> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

impl<'de, T: serde::Deserialize<'de>, E> serde::Deserialize<'de> for ProtoField<T, E> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(ProtoField::new)
    }
}

// ---------------------------------------------------------------------------
// Wire encoding markers (zero-sized)
// ---------------------------------------------------------------------------

/// Standard variable-length encoding (wire type 0).
///
/// Used for: `int32`, `int64`, `uint32`, `uint64`, `bool`, `enum`.
pub struct Varint;

/// ZigZag variable-length encoding (wire type 0).
///
/// More efficient for negative values. Used for: `sint32`, `sint64`.
pub struct ZigZag;

/// Fixed-width encoding (wire type 1 for 64-bit, wire type 5 for 32-bit).
///
/// Used for: `fixed32`, `sfixed32`, `fixed64`, `sfixed64`, `float`, `double`.
pub struct Fixed;

/// Packed repeated encoding (wire type 2, length-delimited).
///
/// All elements are packed into a single length-delimited field instead
/// of each having its own tag. More compact and faster for repeated scalars.
///
/// The inner `E` specifies how individual elements are encoded.
pub struct Packed<E>(PhantomData<E>);

// ---------------------------------------------------------------------------
// Convenience type aliases
// ---------------------------------------------------------------------------

/// A `uint32` field with standard varint encoding.
pub type Uint32 = ProtoField<u32, Varint>;
/// A `uint64` field with standard varint encoding.
pub type Uint64 = ProtoField<u64, Varint>;
/// A `int32` field with standard varint encoding.
pub type Int32 = ProtoField<i32, Varint>;
/// A `int64` field with standard varint encoding.
pub type Int64 = ProtoField<i64, Varint>;
/// A `sint32` field with ZigZag encoding (efficient for negative values).
pub type Sint32 = ProtoField<i32, ZigZag>;
/// A `sint64` field with ZigZag encoding.
pub type Sint64 = ProtoField<i64, ZigZag>;
/// A `fixed32` field with fixed-width encoding.
pub type Fixed32 = ProtoField<u32, Fixed>;
/// A `fixed64` field with fixed-width encoding.
pub type Fixed64 = ProtoField<u64, Fixed>;
/// A `sfixed32` field with fixed-width signed encoding.
pub type Sfixed32 = ProtoField<i32, Fixed>;
/// A `sfixed64` field with fixed-width signed encoding.
pub type Sfixed64 = ProtoField<i64, Fixed>;

/// Packed repeated `uint32` field.
pub type PackedUint32 = ProtoField<Vec<u32>, Packed<Varint>>;
/// Packed repeated `int32` field.
pub type PackedInt32 = ProtoField<Vec<i32>, Packed<Varint>>;
/// Packed repeated `sint32` field.
pub type PackedSint32 = ProtoField<Vec<i32>, Packed<ZigZag>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto_field_new_and_deref() {
        let f = ProtoField::<u32, Varint>::new(42);
        assert_eq!(*f, 42);
        assert_eq!(f.into_inner(), 42);
    }

    #[test]
    fn proto_field_default() {
        let f = Uint32::default();
        assert_eq!(*f, 0);
    }

    #[test]
    fn proto_field_from() {
        let f: Uint32 = 42u32.into();
        assert_eq!(*f, 42);
    }

    #[test]
    fn distinct_types() {
        // These are all i32, but different types at compile time.
        let _varint: ProtoField<i32, Varint> = ProtoField::new(1);
        let _zigzag: ProtoField<i32, ZigZag> = ProtoField::new(1);
        let _fixed: ProtoField<i32, Fixed> = ProtoField::new(1);

        // Verify they're distinct (this is a compile-time check).
        fn takes_varint(_: ProtoField<i32, Varint>) {}
        fn takes_zigzag(_: ProtoField<i32, ZigZag>) {}
        takes_varint(_varint);
        takes_zigzag(_zigzag);
    }

    #[test]
    fn serde_roundtrip() {
        let f: Uint32 = 42u32.into();
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, "42");
        let deserialized: Uint32 = serde_json::from_str(&json).unwrap();
        assert_eq!(*deserialized, 42);
    }

    #[test]
    fn debug_display() {
        let f: Uint32 = 42u32.into();
        assert_eq!(format!("{f:?}"), "42");
        assert_eq!(format!("{f}"), "42");
    }

    #[test]
    fn type_aliases_compile() {
        let _: Uint32 = 0u32.into();
        let _: Uint64 = 0u64.into();
        let _: Int32 = 0i32.into();
        let _: Int64 = 0i64.into();
        let _: Sint32 = 0i32.into();
        let _: Sint64 = 0i64.into();
        let _: Fixed32 = 0u32.into();
        let _: Fixed64 = 0u64.into();
        let _: Sfixed32 = 0i32.into();
        let _: Sfixed64 = 0i64.into();
    }
}
