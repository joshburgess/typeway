//! Typeway native protobuf codec — compile-time specialized encoding.
//!
//! The [`TypewayEncode`] and [`TypewayDecode`] traits provide protobuf
//! binary encoding/decoding with zero runtime dispatch. Unlike the
//! hand-written codec (which operates on `serde_json::Value` with field
//! definitions) or prost (which uses trait objects for message encoding),
//! the Typeway codec generates a specialized encode/decode function for
//! each message type via `#[derive(TypewayCodec)]`.
//!
//! # Performance characteristics
//!
//! - **No runtime field lookup**: tag numbers and wire types are compile-time constants
//! - **Pre-computed buffer size**: `encoded_len()` avoids reallocation during encoding
//! - **No JSON intermediate**: works directly on Rust struct fields
//! - **Inlineable**: the generated code is a sequence of direct writes
//!
//! # Example
//!
//! ```ignore
//! use typeway_macros::TypewayCodec;
//!
//! #[derive(TypewayCodec)]
//! struct User {
//!     #[proto(tag = 1)]
//!     id: u32,
//!     #[proto(tag = 2)]
//!     name: String,
//! }
//!
//! let user = User { id: 42, name: "Alice".into() };
//! let bytes = user.encode_to_vec();
//! let decoded = User::typeway_decode(&bytes).unwrap();
//! assert_eq!(decoded.id, 42);
//! assert_eq!(decoded.name, "Alice");
//! ```

/// Encode a Rust struct as protobuf binary.
///
/// Implemented by `#[derive(TypewayCodec)]`. The generated code is
/// a direct, inlined sequence of field writes with no runtime dispatch.
pub trait TypewayEncode {
    /// Compute the exact encoded size in bytes.
    ///
    /// This allows pre-allocating the output buffer to avoid reallocation.
    fn encoded_len(&self) -> usize;

    /// Encode into an existing buffer.
    ///
    /// The buffer is extended (not cleared). Call `encoded_len()` first
    /// to reserve capacity.
    fn encode_to(&self, buf: &mut Vec<u8>);

    /// Encode to a new `Vec<u8>`.
    fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.encoded_len());
        self.encode_to(&mut buf);
        buf
    }
}

/// Decode a Rust struct from protobuf binary.
///
/// Implemented by `#[derive(TypewayCodec)]`. The generated code
/// parses fields by tag number with direct assignment.
pub trait TypewayDecode: Sized {
    /// Decode from protobuf binary bytes.
    fn typeway_decode(bytes: &[u8]) -> Result<Self, TypewayDecodeError>;

    /// Decode from `Bytes`, enabling zero-copy string fields.
    ///
    /// When string fields use `BytesStr`, this method produces them by
    /// slicing the input `Bytes` (refcount increment, no copy). Falls
    /// back to `typeway_decode` by default.
    fn typeway_decode_bytes(bytes: bytes::Bytes) -> Result<Self, TypewayDecodeError> {
        Self::typeway_decode(&bytes)
    }
}

/// Error from decoding a protobuf message.
#[derive(Debug, Clone)]
pub enum TypewayDecodeError {
    /// Input ended before a complete field could be read.
    UnexpectedEof,
    /// A varint exceeded the maximum of 10 bytes.
    VarintTooLong,
    /// An unknown wire type was encountered.
    UnknownWireType(u8),
    /// A field value could not be converted to the expected Rust type.
    InvalidFieldValue {
        field: &'static str,
        message: String,
    },
    /// A required field was missing (not set and no default).
    MissingField(&'static str),
    /// UTF-8 validation failed for a string field.
    InvalidUtf8(&'static str),
}

impl std::fmt::Display for TypewayDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::VarintTooLong => write!(f, "varint exceeds 10 bytes"),
            Self::UnknownWireType(wt) => write!(f, "unknown wire type: {wt}"),
            Self::InvalidFieldValue { field, message } => {
                write!(f, "invalid value for field '{field}': {message}")
            }
            Self::MissingField(field) => write!(f, "missing required field: {field}"),
            Self::InvalidUtf8(field) => write!(f, "invalid UTF-8 in field: {field}"),
        }
    }
}

impl std::error::Error for TypewayDecodeError {}

// ---------------------------------------------------------------------------
// Encoding helpers (used by generated code)
// ---------------------------------------------------------------------------

/// Encode a varint to a stack buffer.
///
/// Returns `(bytes, length)`. Use with `buf.extend_from_slice(&bytes[..len])`.
/// This avoids per-byte `push` calls on the output Vec.
#[inline]
pub fn tw_encode_varint_array(value: u64) -> ([u8; 10], usize) {
    let mut arr = [0u8; 10];
    let mut i = 0;
    let mut v = value;
    loop {
        if v < 0x80 {
            arr[i] = v as u8;
            return (arr, i + 1);
        }
        arr[i] = (v as u8 & 0x7F) | 0x80;
        v >>= 7;
        i += 1;
    }
}

/// Encode a varint to a buffer.
///
/// Fast path for single-byte values (< 128) avoids reserve entirely.
/// Multi-byte values write directly to spare capacity.
#[inline]
pub fn tw_encode_varint(buf: &mut Vec<u8>, value: u64) {
    // Fast path: single byte (most field tags, small values, booleans).
    if value < 0x80 {
        buf.push(value as u8);
        return;
    }
    // Multi-byte: reserve + unsafe write with single set_len.
    buf.reserve(10);
    let mut pos = buf.len();
    let mut v = value;
    unsafe {
        let base = buf.as_mut_ptr();
        while v >= 0x80 {
            *base.add(pos) = (v as u8 & 0x7F) | 0x80;
            v >>= 7;
            pos += 1;
        }
        *base.add(pos) = v as u8;
        buf.set_len(pos + 1);
    }
}

/// Encode a batch of u32 values as packed varints using direct unsafe writes.
///
/// Reserves worst-case capacity (5 bytes per value), then writes all varints
/// without per-byte bounds checks. This is the hot path for packed repeated
/// uint32/int32 fields.
#[inline]
pub fn tw_encode_packed_u32(buf: &mut Vec<u8>, values: &[u32]) {
    buf.reserve(values.len() * 5);
    let mut pos = buf.len();
    // Safety: we reserved enough capacity for the worst case.
    unsafe {
        let base = buf.as_mut_ptr();
        for &val in values {
            let mut v = val as u64;
            loop {
                if v < 0x80 {
                    *base.add(pos) = v as u8;
                    pos += 1;
                    break;
                }
                *base.add(pos) = (v as u8 & 0x7F) | 0x80;
                pos += 1;
                v >>= 7;
            }
        }
        buf.set_len(pos);
    }
}

/// Encode a varint to a buffer that is known to have enough spare capacity.
///
/// # Safety
/// The caller must ensure `buf` has at least 10 bytes of spare capacity.
#[inline]
pub unsafe fn tw_encode_varint_unchecked(buf: &mut Vec<u8>, mut value: u64) {
    let mut pos = buf.len();
    let base = buf.as_mut_ptr();
    while value >= 0x80 {
        *base.add(pos) = (value as u8 & 0x7F) | 0x80;
        value >>= 7;
        pos += 1;
    }
    *base.add(pos) = value as u8;
    buf.set_len(pos + 1);
}

/// Compute the encoded length of a varint.
#[inline]
pub fn tw_varint_len(value: u64) -> usize {
    if value == 0 {
        return 1;
    }
    let bits = 64 - value.leading_zeros() as usize;
    bits.div_ceil(7)
}

/// Encode a tag (field_number << 3 | wire_type) as a varint.
#[inline]
pub fn tw_encode_tag(buf: &mut Vec<u8>, field_number: u32, wire_type: u8) {
    tw_encode_varint(buf, ((field_number as u64) << 3) | (wire_type as u64));
}

/// Compute the encoded length of a tag.
#[inline]
pub fn tw_tag_len(field_number: u32) -> usize {
    tw_varint_len((field_number as u64) << 3)
}

/// ZigZag encode a signed integer (for sint32/sint64).
#[inline]
pub fn tw_zigzag_encode(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

// ---------------------------------------------------------------------------
// Decoding helpers (used by generated code)
// ---------------------------------------------------------------------------

/// Decode a varint from a byte slice.
///
/// Returns `(value, bytes_consumed)`.
/// Optimized with fast paths for 1-byte and 2-byte varints.
#[inline]
pub fn tw_decode_varint(bytes: &[u8]) -> Result<(u64, usize), TypewayDecodeError> {
    if bytes.is_empty() {
        return Err(TypewayDecodeError::UnexpectedEof);
    }
    // Fast path: single byte (< 128). Most field tags and small values.
    let b0 = bytes[0];
    if b0 < 0x80 {
        return Ok((b0 as u64, 1));
    }
    // Fast path: two bytes (< 16384).
    if bytes.len() < 2 {
        return Err(TypewayDecodeError::UnexpectedEof);
    }
    let b1 = bytes[1];
    if b1 < 0x80 {
        let value = ((b0 & 0x7F) as u64) | ((b1 as u64) << 7);
        return Ok((value, 2));
    }
    // General case: loop for 3+ byte varints.
    let mut value = ((b0 & 0x7F) as u64) | (((b1 & 0x7F) as u64) << 7);
    let mut shift: u32 = 14;
    for (i, &byte) in bytes[2..].iter().enumerate() {
        if i + 2 >= 10 {
            return Err(TypewayDecodeError::VarintTooLong);
        }
        value |= ((byte & 0x7F) as u64) << shift;
        if byte < 0x80 {
            return Ok((value, i + 3));
        }
        shift += 7;
    }
    Err(TypewayDecodeError::UnexpectedEof)
}

/// ZigZag decode an unsigned integer to signed (for sint32/sint64).
#[inline]
pub fn tw_zigzag_decode(value: u64) -> i64 {
    ((value >> 1) as i64) ^ (-((value & 1) as i64))
}

/// Skip a wire value by wire type, returning bytes consumed.
#[inline]
pub fn tw_skip_wire_value(
    bytes: &[u8],
    wire_type: u8,
) -> Result<usize, TypewayDecodeError> {
    match wire_type {
        0 => {
            // Varint — scan for terminator.
            let (_, consumed) = tw_decode_varint(bytes)?;
            Ok(consumed)
        }
        1 => {
            // 64-bit fixed.
            if bytes.len() < 8 {
                return Err(TypewayDecodeError::UnexpectedEof);
            }
            Ok(8)
        }
        2 => {
            // Length-delimited.
            let (len, hdr) = tw_decode_varint(bytes)?;
            let total = hdr + len as usize;
            if bytes.len() < total {
                return Err(TypewayDecodeError::UnexpectedEof);
            }
            Ok(total)
        }
        5 => {
            // 32-bit fixed.
            if bytes.len() < 4 {
                return Err(TypewayDecodeError::UnexpectedEof);
            }
            Ok(4)
        }
        wt => Err(TypewayDecodeError::UnknownWireType(wt)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip_zero() {
        let mut buf = Vec::new();
        tw_encode_varint(&mut buf, 0);
        let (val, consumed) = tw_decode_varint(&buf).unwrap();
        assert_eq!(val, 0);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn varint_roundtrip_small() {
        let mut buf = Vec::new();
        tw_encode_varint(&mut buf, 127);
        assert_eq!(buf.len(), 1);
        let (val, _) = tw_decode_varint(&buf).unwrap();
        assert_eq!(val, 127);
    }

    #[test]
    fn varint_roundtrip_large() {
        let mut buf = Vec::new();
        tw_encode_varint(&mut buf, 300);
        assert_eq!(buf.len(), 2);
        let (val, consumed) = tw_decode_varint(&buf).unwrap();
        assert_eq!(val, 300);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn varint_roundtrip_u64_max() {
        let mut buf = Vec::new();
        tw_encode_varint(&mut buf, u64::MAX);
        let (val, _) = tw_decode_varint(&buf).unwrap();
        assert_eq!(val, u64::MAX);
    }

    #[test]
    fn varint_len_values() {
        assert_eq!(tw_varint_len(0), 1);
        assert_eq!(tw_varint_len(1), 1);
        assert_eq!(tw_varint_len(127), 1);
        assert_eq!(tw_varint_len(128), 2);
        assert_eq!(tw_varint_len(16383), 2);
        assert_eq!(tw_varint_len(16384), 3);
    }

    #[test]
    fn zigzag_roundtrip() {
        for v in [-1i64, 0, 1, -2, 2, i64::MIN, i64::MAX] {
            assert_eq!(tw_zigzag_decode(tw_zigzag_encode(v)), v);
        }
    }

    #[test]
    fn tag_encoding() {
        let mut buf = Vec::new();
        tw_encode_tag(&mut buf, 1, 0); // field 1, varint
        assert_eq!(buf, vec![0x08]);

        buf.clear();
        tw_encode_tag(&mut buf, 2, 2); // field 2, length-delimited
        assert_eq!(buf, vec![0x12]);
    }

    #[test]
    fn skip_varint() {
        let mut buf = Vec::new();
        tw_encode_varint(&mut buf, 300);
        assert_eq!(tw_skip_wire_value(&buf, 0).unwrap(), 2);
    }

    #[test]
    fn skip_fixed64() {
        let buf = [0u8; 8];
        assert_eq!(tw_skip_wire_value(&buf, 1).unwrap(), 8);
    }

    #[test]
    fn skip_length_delimited() {
        let mut buf = Vec::new();
        tw_encode_varint(&mut buf, 5); // length = 5
        buf.extend_from_slice(b"hello");
        assert_eq!(tw_skip_wire_value(&buf, 2).unwrap(), 6); // 1 byte header + 5 bytes
    }

    #[test]
    fn skip_fixed32() {
        let buf = [0u8; 4];
        assert_eq!(tw_skip_wire_value(&buf, 5).unwrap(), 4);
    }

    // Manual encode/decode test to verify the helpers work for
    // a simple message: { id: u32 (tag 1), name: String (tag 2) }
    #[test]
    fn manual_encode_decode_roundtrip() {
        let id: u32 = 42;
        let name = "Alice";

        // Encode.
        let mut buf = Vec::new();
        tw_encode_tag(&mut buf, 1, 0); // field 1, varint
        tw_encode_varint(&mut buf, id as u64);
        tw_encode_tag(&mut buf, 2, 2); // field 2, length-delimited
        tw_encode_varint(&mut buf, name.len() as u64);
        buf.extend_from_slice(name.as_bytes());

        // Decode.
        let mut offset = 0;
        let mut decoded_id: u32 = 0;
        let mut decoded_name = String::new();

        while offset < buf.len() {
            let (tag_wire, consumed) = tw_decode_varint(&buf[offset..]).unwrap();
            offset += consumed;
            let field_number = (tag_wire >> 3) as u32;
            let wire_type = (tag_wire & 0x07) as u8;

            match field_number {
                1 => {
                    assert_eq!(wire_type, 0);
                    let (val, consumed) = tw_decode_varint(&buf[offset..]).unwrap();
                    offset += consumed;
                    decoded_id = val as u32;
                }
                2 => {
                    assert_eq!(wire_type, 2);
                    let (len, consumed) = tw_decode_varint(&buf[offset..]).unwrap();
                    offset += consumed;
                    decoded_name =
                        String::from_utf8(buf[offset..offset + len as usize].to_vec()).unwrap();
                    offset += len as usize;
                }
                _ => {
                    offset += tw_skip_wire_value(&buf[offset..], wire_type).unwrap();
                }
            }
        }

        assert_eq!(decoded_id, 42);
        assert_eq!(decoded_name, "Alice");
    }
}
