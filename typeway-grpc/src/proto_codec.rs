//! Minimal protobuf binary codec for JSON-to-proto transcoding.
//!
//! Implements proto3 wire format encoding and decoding for the scalar types
//! supported by [`ToProtoType`](crate::mapping::ToProtoType), without
//! requiring `prost` or `prost-reflect` as dependencies.
//!
//! The codec works with [`ProtoFieldDef`] field definitions (derived from
//! [`FieldSpec`](crate::spec::FieldSpec)) to know how to encode/decode
//! each field. Nested messages are supported via recursive calls with their
//! own field definitions.
//!
//! # Wire format
//!
//! Proto3 binary encoding uses a tag-length-value scheme:
//!
//! - **Tag**: `(field_number << 3) | wire_type` encoded as a varint
//! - **Wire type 0** (varint): int32, int64, uint32, uint64, sint32, sint64, bool, enum
//! - **Wire type 1** (64-bit): double, fixed64, sfixed64
//! - **Wire type 2** (length-delimited): string, bytes, embedded messages
//! - **Wire type 5** (32-bit): float, fixed32, sfixed32
//!
//! # Limitations
//!
//! - Nested messages are encoded as JSON bytes within a length-delimited
//!   field unless a `MessageFieldResolver` is provided.
//! - `sint32` and `sint64` ZigZag encoding is supported for decoding but
//!   fields are encoded as plain varints unless the proto type is explicitly
//!   `sint32`/`sint64`.
//! - `oneof` fields are not supported.
//! - Map fields are encoded as repeated key-value pair messages per the
//!   proto3 spec.

/// A field definition used for transcoding between JSON and protobuf binary.
///
/// Mirrors [`FieldSpec`](crate::spec::FieldSpec) but is a simpler struct
/// without serde derives, used at the codec level.
#[derive(Debug, Clone)]
pub struct ProtoFieldDef {
    /// Field name (matches the JSON object key).
    pub name: String,
    /// Protobuf type name (e.g., `"string"`, `"uint32"`, `"User"`).
    pub proto_type: String,
    /// Field tag number (1-based, unique within the message).
    pub tag: u32,
    /// Whether this field is `repeated`.
    pub repeated: bool,
    /// Whether this is a `map<K, V>` field.
    pub is_map: bool,
    /// Map key type, if `is_map` is true.
    pub map_key_type: Option<String>,
    /// Map value type, if `is_map` is true.
    pub map_value_type: Option<String>,
    /// Nested field definitions for message-typed fields.
    ///
    /// When present, the codec uses these to recursively encode/decode
    /// nested messages in binary proto format instead of falling back
    /// to JSON bytes.
    pub nested_fields: Option<Vec<ProtoFieldDef>>,
}

/// Errors from protobuf binary encoding or decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// A varint exceeded 10 bytes (64-bit maximum).
    VarintTooLong,
    /// The input ended unexpectedly while reading a value.
    UnexpectedEof,
    /// A wire type value was not recognized (must be 0, 1, 2, or 5).
    UnknownWireType(u8),
    /// A field value could not be converted to the expected JSON type.
    ValueConversion(String),
    /// A JSON value could not be encoded as the expected proto type.
    EncodingFailed(String),
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VarintTooLong => write!(f, "varint exceeds 10 bytes"),
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::UnknownWireType(wt) => write!(f, "unknown wire type: {wt}"),
            Self::ValueConversion(msg) => write!(f, "value conversion error: {msg}"),
            Self::EncodingFailed(msg) => write!(f, "encoding failed: {msg}"),
        }
    }
}

impl std::error::Error for CodecError {}

// ---------------------------------------------------------------------------
// Varint encoding / decoding
// ---------------------------------------------------------------------------

/// Encode a `u64` as a protobuf varint and append to `buf`.
pub fn encode_varint(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            buf.push(byte);
            break;
        } else {
            buf.push(byte | 0x80);
        }
    }
}

/// Decode a protobuf varint from `bytes`.
///
/// Returns `(value, bytes_consumed)`.
pub fn decode_varint(bytes: &[u8]) -> Result<(u64, usize), CodecError> {
    let mut result: u64 = 0;
    let mut shift = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if shift >= 70 {
            return Err(CodecError::VarintTooLong);
        }
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok((result, i + 1));
        }
        shift += 7;
    }
    Err(CodecError::UnexpectedEof)
}

/// Apply ZigZag decoding to a varint-decoded value.
///
/// Used for `sint32` and `sint64` fields.
fn zigzag_decode(v: u64) -> i64 {
    ((v >> 1) as i64) ^ (-((v & 1) as i64))
}

/// Apply ZigZag encoding to a signed value.
///
/// Used for `sint32` and `sint64` fields.
fn zigzag_encode(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

// ---------------------------------------------------------------------------
// Wire type helpers
// ---------------------------------------------------------------------------

/// Return the wire type for a given proto type name.
///
/// Proto3 wire types:
/// - 0: varint (int32, int64, uint32, uint64, sint32, sint64, bool, enum)
/// - 1: 64-bit (double, fixed64, sfixed64)
/// - 2: length-delimited (string, bytes, messages, repeated packed)
/// - 5: 32-bit (float, fixed32, sfixed32)
pub fn wire_type_for(proto_type: &str) -> u8 {
    match proto_type {
        "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" | "bool" | "enum" => 0,
        "double" | "fixed64" | "sfixed64" => 1,
        "string" | "bytes" => 2,
        "float" | "fixed32" | "sfixed32" => 5,
        _ => 2, // message types are length-delimited
    }
}

/// Return `true` if a JSON value is the proto3 default for the given type.
///
/// Proto3 default values: 0 for numeric types, `false` for bool,
/// `""` for string, empty for bytes. Default values should not be
/// encoded on the wire.
fn is_proto3_default(value: &serde_json::Value, proto_type: &str) -> bool {
    match proto_type {
        "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" | "fixed32" | "fixed64"
        | "sfixed32" | "sfixed64" | "enum" => {
            value.as_i64() == Some(0) || value.as_u64() == Some(0) || value.as_f64() == Some(0.0)
        }
        "float" | "double" => value.as_f64() == Some(0.0),
        "bool" => value.as_bool() == Some(false),
        "string" => value.as_str() == Some(""),
        "bytes" => value.as_str() == Some(""),
        _ => false, // message types: never skip
    }
}

/// Return `true` if the proto type name refers to a scalar (non-message) type.
pub fn is_scalar_type(proto_type: &str) -> bool {
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
    )
}

// ---------------------------------------------------------------------------
// Raw wire value
// ---------------------------------------------------------------------------

/// A decoded wire value before type interpretation.
#[derive(Debug)]
enum WireValue {
    Varint(u64),
    Fixed64([u8; 8]),
    LengthDelimited(Vec<u8>),
    Fixed32([u8; 4]),
}

/// Decode a raw wire value of the given wire type from `bytes`.
///
/// Returns `(value, bytes_consumed)`.
fn decode_wire_value(bytes: &[u8], wire_type: u8) -> Result<(WireValue, usize), CodecError> {
    match wire_type {
        0 => {
            let (val, consumed) = decode_varint(bytes)?;
            Ok((WireValue::Varint(val), consumed))
        }
        1 => {
            if bytes.len() < 8 {
                return Err(CodecError::UnexpectedEof);
            }
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&bytes[..8]);
            Ok((WireValue::Fixed64(arr), 8))
        }
        2 => {
            let (len, hdr_consumed) = decode_varint(bytes)?;
            let len = len as usize;
            if bytes.len() < hdr_consumed + len {
                return Err(CodecError::UnexpectedEof);
            }
            let data = bytes[hdr_consumed..hdr_consumed + len].to_vec();
            Ok((WireValue::LengthDelimited(data), hdr_consumed + len))
        }
        5 => {
            if bytes.len() < 4 {
                return Err(CodecError::UnexpectedEof);
            }
            let mut arr = [0u8; 4];
            arr.copy_from_slice(&bytes[..4]);
            Ok((WireValue::Fixed32(arr), 4))
        }
        _ => Err(CodecError::UnknownWireType(wire_type)),
    }
}

// ---------------------------------------------------------------------------
// Wire value → JSON
// ---------------------------------------------------------------------------

/// Convert a raw [`WireValue`] to a [`serde_json::Value`] based on the proto type.
fn wire_value_to_json(
    wv: &WireValue,
    proto_type: &str,
    nested_fields: Option<&[ProtoFieldDef]>,
) -> Result<serde_json::Value, CodecError> {
    match (wv, proto_type) {
        // Varint types
        (WireValue::Varint(v), "uint32" | "uint64") => Ok(serde_json::json!(*v)),
        (WireValue::Varint(v), "int32") => Ok(serde_json::json!(*v as i32)),
        (WireValue::Varint(v), "int64") => Ok(serde_json::json!(*v as i64)),
        (WireValue::Varint(v), "sint32") => Ok(serde_json::json!(zigzag_decode(*v) as i32)),
        (WireValue::Varint(v), "sint64") => Ok(serde_json::json!(zigzag_decode(*v))),
        (WireValue::Varint(v), "bool") => Ok(serde_json::json!(*v != 0)),
        (WireValue::Varint(v), "enum") => Ok(serde_json::json!(*v)),

        // 64-bit types
        (WireValue::Fixed64(bytes), "double") => Ok(serde_json::json!(f64::from_le_bytes(*bytes))),
        (WireValue::Fixed64(bytes), "fixed64") => Ok(serde_json::json!(u64::from_le_bytes(*bytes))),
        (WireValue::Fixed64(bytes), "sfixed64") => {
            Ok(serde_json::json!(i64::from_le_bytes(*bytes)))
        }

        // 32-bit types
        (WireValue::Fixed32(bytes), "float") => Ok(serde_json::json!(f32::from_le_bytes(*bytes))),
        (WireValue::Fixed32(bytes), "fixed32") => Ok(serde_json::json!(u32::from_le_bytes(*bytes))),
        (WireValue::Fixed32(bytes), "sfixed32") => {
            Ok(serde_json::json!(i32::from_le_bytes(*bytes)))
        }

        // Length-delimited types
        (WireValue::LengthDelimited(data), "string") => {
            let s = String::from_utf8(data.clone()).map_err(|e| {
                CodecError::ValueConversion(format!("invalid UTF-8 in string field: {e}"))
            })?;
            Ok(serde_json::json!(s))
        }
        (WireValue::LengthDelimited(data), "bytes") => {
            use base64_encode::Engine;
            let encoded = base64_encode::engine::general_purpose::STANDARD.encode(data);
            Ok(serde_json::json!(encoded))
        }

        // Nested message — decode recursively if we have field defs
        (WireValue::LengthDelimited(data), _) => {
            if let Some(fields) = nested_fields {
                proto_binary_to_json(data, fields)
            } else {
                // Fallback: try to parse as JSON (for messages encoded with JSON fallback)
                match serde_json::from_slice(data) {
                    Ok(val) => Ok(val),
                    Err(_) => {
                        // Return raw bytes as base64
                        use base64_encode::Engine;
                        let encoded = base64_encode::engine::general_purpose::STANDARD.encode(data);
                        Ok(serde_json::json!(encoded))
                    }
                }
            }
        }

        _ => Err(CodecError::ValueConversion(format!(
            "cannot convert wire value to JSON for proto type '{proto_type}'"
        ))),
    }
}

// ---------------------------------------------------------------------------
// JSON → protobuf binary
// ---------------------------------------------------------------------------

/// Encode a JSON object as protobuf binary using the given field definitions.
///
/// Each field in the JSON object is matched by name against the field
/// definitions. Fields present in the JSON but missing from the definitions
/// are silently skipped. Fields with proto3 default values (zero, empty
/// string, false) are omitted from the output per proto3 conventions.
///
/// # Example
///
/// ```
/// use typeway_grpc::proto_codec::{ProtoFieldDef, json_to_proto_binary};
///
/// let fields = vec![
///     ProtoFieldDef {
///         name: "id".into(),
///         proto_type: "uint32".into(),
///         tag: 1,
///         repeated: false,
///         is_map: false,
///         map_key_type: None,
///         map_value_type: None,
///         nested_fields: None,
///     },
///     ProtoFieldDef {
///         name: "name".into(),
///         proto_type: "string".into(),
///         tag: 2,
///         repeated: false,
///         is_map: false,
///         map_key_type: None,
///         map_value_type: None,
///         nested_fields: None,
///     },
/// ];
///
/// let json = serde_json::json!({"id": 42, "name": "Alice"});
/// let bytes = json_to_proto_binary(&json, &fields).unwrap();
/// assert!(!bytes.is_empty());
/// ```
pub fn json_to_proto_binary(
    json: &serde_json::Value,
    fields: &[ProtoFieldDef],
) -> Result<Vec<u8>, CodecError> {
    let mut buf = Vec::new();

    let map = match json.as_object() {
        Some(m) => m,
        None => return Ok(buf), // non-object → empty message
    };

    for field in fields {
        let value = match map.get(&field.name) {
            Some(v) => v,
            None => continue,
        };

        // Skip null values.
        if value.is_null() {
            continue;
        }

        // Proto3: skip default values (0, false, "", empty bytes).
        if !field.repeated && !field.is_map && is_proto3_default(value, &field.proto_type) {
            continue;
        }

        if field.is_map {
            // Proto3 map: encode as repeated submessages with field 1 = key, field 2 = value.
            if let Some(obj) = value.as_object() {
                let key_type = field.map_key_type.as_deref().unwrap_or("string");
                let val_type = field.map_value_type.as_deref().unwrap_or("string");
                for (k, v) in obj {
                    let mut entry_buf = Vec::new();
                    // Encode key as field 1
                    let key_field = ProtoFieldDef {
                        name: "key".into(),
                        proto_type: key_type.into(),
                        tag: 1,
                        repeated: false,
                        is_map: false,
                        map_key_type: None,
                        map_value_type: None,
                        nested_fields: None,
                    };
                    encode_field(&mut entry_buf, &key_field, &serde_json::json!(k))?;
                    // Encode value as field 2
                    let val_field = ProtoFieldDef {
                        name: "value".into(),
                        proto_type: val_type.into(),
                        tag: 2,
                        repeated: false,
                        is_map: false,
                        map_key_type: None,
                        map_value_type: None,
                        nested_fields: None,
                    };
                    encode_field(&mut entry_buf, &val_field, v)?;
                    // Write as length-delimited submessage
                    let tag_value = ((field.tag as u64) << 3) | 2;
                    encode_varint(&mut buf, tag_value);
                    encode_varint(&mut buf, entry_buf.len() as u64);
                    buf.extend_from_slice(&entry_buf);
                }
            }
        } else if field.repeated {
            if let Some(arr) = value.as_array() {
                for item in arr {
                    encode_field(&mut buf, field, item)?;
                }
            }
        } else {
            encode_field(&mut buf, field, value)?;
        }
    }

    Ok(buf)
}

/// Encode a single field (tag + value) into the buffer.
fn encode_field(
    buf: &mut Vec<u8>,
    field: &ProtoFieldDef,
    value: &serde_json::Value,
) -> Result<(), CodecError> {
    let wt = wire_type_for(&field.proto_type);
    let tag_value = ((field.tag as u64) << 3) | (wt as u64);

    match wt {
        0 => {
            // Varint
            let v = json_to_varint(value, &field.proto_type)?;
            encode_varint(buf, tag_value);
            encode_varint(buf, v);
        }
        1 => {
            // 64-bit fixed
            encode_varint(buf, tag_value);
            match field.proto_type.as_str() {
                "double" => {
                    let n = value.as_f64().ok_or_else(|| {
                        CodecError::EncodingFailed("expected number for double".into())
                    })?;
                    buf.extend_from_slice(&n.to_le_bytes());
                }
                "fixed64" => {
                    let n = json_as_u64(value)?;
                    buf.extend_from_slice(&n.to_le_bytes());
                }
                "sfixed64" => {
                    let n = json_as_i64(value)?;
                    buf.extend_from_slice(&n.to_le_bytes());
                }
                _ => {
                    return Err(CodecError::EncodingFailed(format!(
                        "unhandled 64-bit type: {}",
                        field.proto_type
                    )));
                }
            }
        }
        2 => {
            // Length-delimited
            encode_varint(buf, tag_value);
            match field.proto_type.as_str() {
                "string" => {
                    let s = value.as_str().ok_or_else(|| {
                        CodecError::EncodingFailed("expected string for string field".into())
                    })?;
                    encode_varint(buf, s.len() as u64);
                    buf.extend_from_slice(s.as_bytes());
                }
                "bytes" => {
                    // Accept both base64-encoded strings and raw strings.
                    let data = if let Some(s) = value.as_str() {
                        use base64_encode::Engine;
                        base64_encode::engine::general_purpose::STANDARD
                            .decode(s)
                            .unwrap_or_else(|_| s.as_bytes().to_vec())
                    } else {
                        return Err(CodecError::EncodingFailed(
                            "expected string (base64) for bytes field".into(),
                        ));
                    };
                    encode_varint(buf, data.len() as u64);
                    buf.extend_from_slice(&data);
                }
                _ => {
                    // Nested message type — encode recursively if we have field defs.
                    if let Some(ref nested) = field.nested_fields {
                        let nested_bytes = json_to_proto_binary(value, nested)?;
                        encode_varint(buf, nested_bytes.len() as u64);
                        buf.extend_from_slice(&nested_bytes);
                    } else {
                        // Fallback: encode the JSON as bytes.
                        let json_bytes = serde_json::to_vec(value).map_err(|e| {
                            CodecError::EncodingFailed(format!("JSON serialization failed: {e}"))
                        })?;
                        encode_varint(buf, json_bytes.len() as u64);
                        buf.extend_from_slice(&json_bytes);
                    }
                }
            }
        }
        5 => {
            // 32-bit fixed
            encode_varint(buf, tag_value);
            match field.proto_type.as_str() {
                "float" => {
                    let n = value.as_f64().ok_or_else(|| {
                        CodecError::EncodingFailed("expected number for float".into())
                    })? as f32;
                    buf.extend_from_slice(&n.to_le_bytes());
                }
                "fixed32" => {
                    let n = json_as_u64(value)? as u32;
                    buf.extend_from_slice(&n.to_le_bytes());
                }
                "sfixed32" => {
                    let n = json_as_i64(value)? as i32;
                    buf.extend_from_slice(&n.to_le_bytes());
                }
                _ => {
                    return Err(CodecError::EncodingFailed(format!(
                        "unhandled 32-bit type: {}",
                        field.proto_type
                    )));
                }
            }
        }
        _ => {
            return Err(CodecError::EncodingFailed(format!(
                "unknown wire type {wt} for field '{}'",
                field.name
            )));
        }
    }

    Ok(())
}

/// Convert a JSON value to a varint `u64` for the given proto type.
fn json_to_varint(value: &serde_json::Value, proto_type: &str) -> Result<u64, CodecError> {
    match proto_type {
        "bool" => Ok(if value.as_bool().unwrap_or(false) {
            1
        } else {
            0
        }),
        "sint32" => {
            let n = json_as_i64(value)? as i32;
            Ok(zigzag_encode(n as i64))
        }
        "sint64" => {
            let n = json_as_i64(value)?;
            Ok(zigzag_encode(n))
        }
        "int32" | "int64" => {
            let n = json_as_i64(value)?;
            Ok(n as u64)
        }
        _ => json_as_u64(value),
    }
}

/// Extract a `u64` from a JSON number value.
fn json_as_u64(value: &serde_json::Value) -> Result<u64, CodecError> {
    value
        .as_u64()
        .or_else(|| value.as_i64().map(|v| v as u64))
        .or_else(|| value.as_f64().map(|v| v as u64))
        .ok_or_else(|| CodecError::EncodingFailed("expected numeric value".into()))
}

/// Extract an `i64` from a JSON number value.
fn json_as_i64(value: &serde_json::Value) -> Result<i64, CodecError> {
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|v| v as i64))
        .or_else(|| value.as_f64().map(|v| v as i64))
        .ok_or_else(|| CodecError::EncodingFailed("expected numeric value".into()))
}

// ---------------------------------------------------------------------------
// Protobuf binary → JSON
// ---------------------------------------------------------------------------

/// Decode protobuf binary bytes into a JSON object using the given field definitions.
///
/// Unknown fields (present in the binary but not in the definitions) are
/// silently skipped, following proto3 forward-compatibility rules.
///
/// # Example
///
/// ```
/// use typeway_grpc::proto_codec::{ProtoFieldDef, json_to_proto_binary, proto_binary_to_json};
///
/// let fields = vec![
///     ProtoFieldDef {
///         name: "id".into(),
///         proto_type: "uint32".into(),
///         tag: 1,
///         repeated: false,
///         is_map: false,
///         map_key_type: None,
///         map_value_type: None,
///         nested_fields: None,
///     },
/// ];
///
/// let json = serde_json::json!({"id": 7});
/// let bytes = json_to_proto_binary(&json, &fields).unwrap();
/// let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
/// assert_eq!(decoded["id"], 7);
/// ```
pub fn proto_binary_to_json(
    bytes: &[u8],
    fields: &[ProtoFieldDef],
) -> Result<serde_json::Value, CodecError> {
    let mut map = serde_json::Map::new();
    // Track repeated fields for accumulation.
    let mut repeated_fields: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();

    let mut offset = 0;

    while offset < bytes.len() {
        let (tag_and_wire, consumed) = decode_varint(&bytes[offset..])?;
        offset += consumed;

        let field_number = (tag_and_wire >> 3) as u32;
        let wire_type = (tag_and_wire & 0x07) as u8;

        let (wire_val, consumed) = decode_wire_value(&bytes[offset..], wire_type)?;
        offset += consumed;

        // Find the field definition by tag number.
        let field = fields.iter().find(|f| f.tag == field_number);

        if let Some(field) = field {
            // Handle packed repeated fields: a repeated scalar field may arrive
            // as a single length-delimited blob containing concatenated values.
            if field.repeated
                && matches!(wire_val, WireValue::LengthDelimited(_))
                && is_packable_type(&field.proto_type)
            {
                if let WireValue::LengthDelimited(ref data) = wire_val {
                    let unpacked = unpack_scalars(data, &field.proto_type)?;
                    repeated_fields
                        .entry(field.name.clone())
                        .or_default()
                        .extend(unpacked);
                }
            } else {
                let json_value = wire_value_to_json(
                    &wire_val,
                    &field.proto_type,
                    field.nested_fields.as_deref(),
                )?;

                if field.repeated {
                    repeated_fields
                        .entry(field.name.clone())
                        .or_default()
                        .push(json_value);
                } else {
                    map.insert(field.name.clone(), json_value);
                }
            }
        }
        // Unknown fields are silently skipped (proto3 forward compatibility).
    }

    // Insert accumulated repeated fields as arrays.
    for (name, values) in repeated_fields {
        map.insert(name, serde_json::Value::Array(values));
    }

    Ok(serde_json::Value::Object(map))
}

/// Return `true` if the proto type can appear in packed encoding.
fn is_packable_type(proto_type: &str) -> bool {
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
            | "fixed32"
            | "sfixed32"
            | "float"
            | "fixed64"
            | "sfixed64"
            | "double"
    )
}

/// Unpack a packed repeated scalar field from a length-delimited blob.
fn unpack_scalars(data: &[u8], proto_type: &str) -> Result<Vec<serde_json::Value>, CodecError> {
    let mut results = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        match proto_type {
            "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" | "bool" | "enum" => {
                let (val, consumed) = decode_varint(&data[offset..])?;
                offset += consumed;
                let wv = WireValue::Varint(val);
                results.push(wire_value_to_json(&wv, proto_type, None)?);
            }
            "fixed32" | "sfixed32" | "float" => {
                if offset + 4 > data.len() {
                    return Err(CodecError::UnexpectedEof);
                }
                let mut arr = [0u8; 4];
                arr.copy_from_slice(&data[offset..offset + 4]);
                offset += 4;
                let wv = WireValue::Fixed32(arr);
                results.push(wire_value_to_json(&wv, proto_type, None)?);
            }
            "fixed64" | "sfixed64" | "double" => {
                if offset + 8 > data.len() {
                    return Err(CodecError::UnexpectedEof);
                }
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&data[offset..offset + 8]);
                offset += 8;
                let wv = WireValue::Fixed64(arr);
                results.push(wire_value_to_json(&wv, proto_type, None)?);
            }
            _ => {
                return Err(CodecError::EncodingFailed(format!(
                    "cannot unpack type: {proto_type}"
                )))
            }
        }
    }
    Ok(results)
}

// ---------------------------------------------------------------------------
// base64 encoding helpers (inline, no external dependency)
// ---------------------------------------------------------------------------

/// Minimal base64 encode/decode used for `bytes` fields.
///
/// We use a submodule alias so the codec does not require an external
/// base64 crate. The implementation is standard RFC 4648 base64.
mod base64_encode {
    pub trait Engine {
        fn encode(&self, input: &[u8]) -> String;
        fn decode(&self, input: &str) -> Result<Vec<u8>, DecodeError>;
    }

    pub mod engine {
        pub mod general_purpose {
            pub const STANDARD: StandardEngine = StandardEngine;

            pub struct StandardEngine;

            impl super::super::Engine for StandardEngine {
                fn encode(&self, input: &[u8]) -> String {
                    const CHARS: &[u8; 64] =
                        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
                    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
                    for chunk in input.chunks(3) {
                        let b0 = chunk[0] as u32;
                        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
                        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
                        let triple = (b0 << 16) | (b1 << 8) | b2;
                        out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
                        out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
                        if chunk.len() > 1 {
                            out.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
                        } else {
                            out.push('=');
                        }
                        if chunk.len() > 2 {
                            out.push(CHARS[(triple & 0x3F) as usize] as char);
                        } else {
                            out.push('=');
                        }
                    }
                    out
                }

                fn decode(&self, input: &str) -> Result<Vec<u8>, super::super::DecodeError> {
                    let input = input.trim_end_matches('=');
                    let mut out = Vec::with_capacity(input.len() * 3 / 4);
                    let mut buf: u32 = 0;
                    let mut bits: u32 = 0;
                    for c in input.chars() {
                        let val = match c {
                            'A'..='Z' => (c as u32) - ('A' as u32),
                            'a'..='z' => (c as u32) - ('a' as u32) + 26,
                            '0'..='9' => (c as u32) - ('0' as u32) + 52,
                            '+' => 62,
                            '/' => 63,
                            _ => return Err(super::super::DecodeError),
                        };
                        buf = (buf << 6) | val;
                        bits += 6;
                        if bits >= 8 {
                            bits -= 8;
                            out.push((buf >> bits) as u8);
                            buf &= (1 << bits) - 1;
                        }
                    }
                    Ok(out)
                }
            }
        }
    }

    #[derive(Debug)]
    pub struct DecodeError;
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Varint tests ---

    #[test]
    fn varint_roundtrip_zero() {
        let mut buf = Vec::new();
        encode_varint(&mut buf, 0);
        let (val, consumed) = decode_varint(&buf).unwrap();
        assert_eq!(val, 0);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn varint_roundtrip_small() {
        let mut buf = Vec::new();
        encode_varint(&mut buf, 42);
        let (val, consumed) = decode_varint(&buf).unwrap();
        assert_eq!(val, 42);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn varint_roundtrip_large() {
        let mut buf = Vec::new();
        encode_varint(&mut buf, 300);
        assert_eq!(buf.len(), 2); // 300 needs 2 bytes
        let (val, consumed) = decode_varint(&buf).unwrap();
        assert_eq!(val, 300);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn varint_roundtrip_u64_max() {
        let mut buf = Vec::new();
        encode_varint(&mut buf, u64::MAX);
        let (val, consumed) = decode_varint(&buf).unwrap();
        assert_eq!(val, u64::MAX);
        assert_eq!(consumed, buf.len());
    }

    #[test]
    fn varint_empty_input() {
        assert_eq!(decode_varint(&[]), Err(CodecError::UnexpectedEof));
    }

    // --- ZigZag tests ---

    #[test]
    fn zigzag_roundtrip() {
        for v in [-1i64, 0, 1, -2, 2, i64::MIN, i64::MAX] {
            assert_eq!(zigzag_decode(zigzag_encode(v)), v);
        }
    }

    // --- Wire type tests ---

    #[test]
    fn wire_types() {
        assert_eq!(wire_type_for("uint32"), 0);
        assert_eq!(wire_type_for("int64"), 0);
        assert_eq!(wire_type_for("bool"), 0);
        assert_eq!(wire_type_for("double"), 1);
        assert_eq!(wire_type_for("string"), 2);
        assert_eq!(wire_type_for("bytes"), 2);
        assert_eq!(wire_type_for("float"), 5);
        assert_eq!(wire_type_for("SomeMessage"), 2);
    }

    // --- Helper to build a simple field def ---

    fn scalar_field(name: &str, proto_type: &str, tag: u32) -> ProtoFieldDef {
        ProtoFieldDef {
            name: name.into(),
            proto_type: proto_type.into(),
            tag,
            repeated: false,
            is_map: false,
            map_key_type: None,
            map_value_type: None,
            nested_fields: None,
        }
    }

    // --- Encode/decode field roundtrip tests ---

    #[test]
    fn roundtrip_uint32() {
        let fields = vec![scalar_field("id", "uint32", 1)];
        let json = serde_json::json!({"id": 42});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        assert_eq!(decoded["id"], 42);
    }

    #[test]
    fn roundtrip_int32_negative() {
        let fields = vec![scalar_field("val", "int32", 1)];
        let json = serde_json::json!({"val": -7});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        assert_eq!(decoded["val"], -7);
    }

    #[test]
    fn roundtrip_string() {
        let fields = vec![scalar_field("name", "string", 1)];
        let json = serde_json::json!({"name": "Alice"});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        assert_eq!(decoded["name"], "Alice");
    }

    #[test]
    fn roundtrip_bool() {
        let fields = vec![scalar_field("active", "bool", 1)];
        let json = serde_json::json!({"active": true});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        assert_eq!(decoded["active"], true);
    }

    #[test]
    fn roundtrip_float() {
        let fields = vec![scalar_field("score", "float", 1)];
        let json = serde_json::json!({"score": 1.234});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        // Float precision: 1.234 as f32 = 1.2340000104904175
        let score = decoded["score"].as_f64().unwrap();
        assert!((score - 1.234).abs() < 0.01);
    }

    #[test]
    fn roundtrip_double() {
        let fields = vec![scalar_field("precise", "double", 1)];
        let json = serde_json::json!({"precise": 1.2341592653589793});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        assert_eq!(decoded["precise"], 1.2341592653589793);
    }

    #[test]
    fn roundtrip_multiple_fields() {
        let fields = vec![
            scalar_field("id", "uint32", 1),
            scalar_field("name", "string", 2),
            scalar_field("active", "bool", 3),
        ];
        let json = serde_json::json!({"id": 99, "name": "Bob", "active": true});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        assert_eq!(decoded["id"], 99);
        assert_eq!(decoded["name"], "Bob");
        assert_eq!(decoded["active"], true);
    }

    #[test]
    fn unknown_fields_skipped() {
        // Encode with more fields than we decode with.
        let encode_fields = vec![
            scalar_field("id", "uint32", 1),
            scalar_field("secret", "string", 2),
            scalar_field("name", "string", 3),
        ];
        let decode_fields = vec![
            scalar_field("id", "uint32", 1),
            // field 2 ("secret") is unknown to the decoder
            scalar_field("name", "string", 3),
        ];

        let json = serde_json::json!({"id": 1, "secret": "hidden", "name": "Test"});
        let bytes = json_to_proto_binary(&json, &encode_fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &decode_fields).unwrap();
        assert_eq!(decoded["id"], 1);
        assert_eq!(decoded["name"], "Test");
        assert!(decoded.get("secret").is_none());
    }

    #[test]
    fn empty_message_encodes_to_empty_bytes() {
        let fields = vec![scalar_field("id", "uint32", 1)];
        let json = serde_json::json!({});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn null_fields_skipped() {
        let fields = vec![
            scalar_field("id", "uint32", 1),
            scalar_field("name", "string", 2),
        ];
        let json = serde_json::json!({"id": 5, "name": null});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        assert_eq!(decoded["id"], 5);
        assert!(decoded.get("name").is_none());
    }

    #[test]
    fn roundtrip_sint32() {
        let fields = vec![scalar_field("val", "sint32", 1)];
        let json = serde_json::json!({"val": -42});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        assert_eq!(decoded["val"], -42);
    }

    #[test]
    fn roundtrip_uint64_large() {
        let fields = vec![scalar_field("big", "uint64", 1)];
        let big_val: u64 = 1_000_000_000_000;
        let json = serde_json::json!({"big": big_val});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        assert_eq!(decoded["big"], big_val);
    }

    #[test]
    fn roundtrip_nested_message() {
        let inner_fields = vec![
            scalar_field("street", "string", 1),
            scalar_field("city", "string", 2),
        ];
        let outer_fields = vec![
            scalar_field("name", "string", 1),
            ProtoFieldDef {
                name: "address".into(),
                proto_type: "Address".into(),
                tag: 2,
                repeated: false,
                is_map: false,
                map_key_type: None,
                map_value_type: None,
                nested_fields: Some(inner_fields),
            },
        ];

        let json = serde_json::json!({
            "name": "Alice",
            "address": {"street": "123 Main St", "city": "Springfield"}
        });
        let bytes = json_to_proto_binary(&json, &outer_fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &outer_fields).unwrap();
        assert_eq!(decoded["name"], "Alice");
        assert_eq!(decoded["address"]["street"], "123 Main St");
        assert_eq!(decoded["address"]["city"], "Springfield");
    }

    #[test]
    fn roundtrip_repeated_field() {
        let fields = vec![ProtoFieldDef {
            name: "ids".into(),
            proto_type: "uint32".into(),
            tag: 1,
            repeated: true,
            is_map: false,
            map_key_type: None,
            map_value_type: None,
            nested_fields: None,
        }];

        let json = serde_json::json!({"ids": [1, 2, 3]});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        assert_eq!(decoded["ids"], serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn non_object_json_encodes_empty() {
        let fields = vec![scalar_field("id", "uint32", 1)];
        let json = serde_json::json!("not an object");
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn decode_empty_bytes_is_empty_object() {
        let fields = vec![scalar_field("id", "uint32", 1)];
        let decoded = proto_binary_to_json(&[], &fields).unwrap();
        assert_eq!(decoded, serde_json::json!({}));
    }
}
