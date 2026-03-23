//! Tests for the protobuf binary codec (`proto_codec` module).
//!
//! These tests verify encoding and decoding of protobuf binary wire format
//! for the types supported by the gRPC bridge transcoder.

use typeway_grpc::proto_codec::{
    decode_varint, encode_varint, json_to_proto_binary, proto_binary_to_json, wire_type_for,
    CodecError, ProtoFieldDef,
};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Varint roundtrip tests
// ---------------------------------------------------------------------------

#[test]
fn encode_decode_varint_roundtrip() {
    let test_values: Vec<u64> = vec![0, 1, 127, 128, 255, 300, 16384, u32::MAX as u64, u64::MAX];

    for &val in &test_values {
        let mut buf = Vec::new();
        encode_varint(&mut buf, val);
        let (decoded, consumed) = decode_varint(&buf).unwrap();
        assert_eq!(decoded, val, "roundtrip failed for value {val}");
        assert_eq!(consumed, buf.len(), "consumed mismatch for value {val}");
    }
}

#[test]
fn varint_error_on_empty() {
    assert!(matches!(decode_varint(&[]), Err(CodecError::UnexpectedEof)));
}

// ---------------------------------------------------------------------------
// Individual field type tests
// ---------------------------------------------------------------------------

#[test]
fn encode_decode_string_field() {
    let fields = vec![scalar_field("greeting", "string", 1)];
    let json = serde_json::json!({"greeting": "hello world"});

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    assert!(!bytes.is_empty());

    let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
    assert_eq!(decoded["greeting"], "hello world");
}

#[test]
fn encode_decode_int_field() {
    let fields = vec![scalar_field("count", "int32", 1)];
    let json = serde_json::json!({"count": -42});

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
    assert_eq!(decoded["count"], -42);
}

#[test]
fn encode_decode_uint32_field() {
    let fields = vec![scalar_field("id", "uint32", 1)];
    let json = serde_json::json!({"id": 12345});

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
    assert_eq!(decoded["id"], 12345);
}

#[test]
fn encode_decode_uint64_field() {
    let fields = vec![scalar_field("big", "uint64", 1)];
    let big: u64 = 9_000_000_000_000;
    let json = serde_json::json!({"big": big});

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
    assert_eq!(decoded["big"], big);
}

#[test]
fn encode_decode_bool_field() {
    let fields = vec![scalar_field("flag", "bool", 1)];

    // true
    let json = serde_json::json!({"flag": true});
    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
    assert_eq!(decoded["flag"], true);

    // false — proto3 default, omitted from wire, absent in decoded JSON
    let json = serde_json::json!({"flag": false});
    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    assert!(bytes.is_empty(), "false is proto3 default, should not be encoded");
    let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
    assert!(decoded["flag"].is_null(), "absent field returns null");
}

#[test]
fn encode_decode_float_field() {
    let fields = vec![scalar_field("score", "float", 1)];
    let json = serde_json::json!({"score": 2.5});

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
    let score = decoded["score"].as_f64().unwrap();
    assert!((score - 2.5).abs() < 0.001);
}

#[test]
fn encode_decode_double_field() {
    let fields = vec![scalar_field("precise", "double", 1)];
    let json = serde_json::json!({"precise": std::f64::consts::PI});

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
    assert_eq!(decoded["precise"], std::f64::consts::PI);
}

// ---------------------------------------------------------------------------
// Multiple fields
// ---------------------------------------------------------------------------

#[test]
fn encode_decode_multiple_fields() {
    let fields = vec![
        scalar_field("id", "uint32", 1),
        scalar_field("name", "string", 2),
        scalar_field("email", "string", 3),
        scalar_field("active", "bool", 4),
        scalar_field("score", "double", 5),
    ];

    let json = serde_json::json!({
        "id": 42,
        "name": "Alice",
        "email": "alice@example.com",
        "active": true,
        "score": 98.6
    });

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let decoded = proto_binary_to_json(&bytes, &fields).unwrap();

    assert_eq!(decoded["id"], 42);
    assert_eq!(decoded["name"], "Alice");
    assert_eq!(decoded["email"], "alice@example.com");
    assert_eq!(decoded["active"], true);
    assert_eq!(decoded["score"], 98.6);
}

// ---------------------------------------------------------------------------
// JSON → proto binary → JSON roundtrip for a simple message
// ---------------------------------------------------------------------------

#[test]
fn json_to_proto_binary_simple_message() {
    let fields = vec![
        scalar_field("user_id", "uint32", 1),
        scalar_field("username", "string", 2),
    ];

    let json = serde_json::json!({"user_id": 7, "username": "bob"});
    let bytes = json_to_proto_binary(&json, &fields).unwrap();

    // Verify the binary is non-empty and well-formed.
    assert!(!bytes.is_empty());

    // The first byte should be a valid tag (field 1, wire type 0 = varint for uint32).
    let (tag, _) = decode_varint(&bytes).unwrap();
    let field_num = tag >> 3;
    let wire_type = tag & 0x07;
    assert_eq!(field_num, 1);
    assert_eq!(wire_type, 0); // varint
}

#[test]
fn proto_binary_to_json_simple_message() {
    let fields = vec![
        scalar_field("id", "uint32", 1),
        scalar_field("name", "string", 2),
    ];

    // Manually encode: field 1 (uint32, tag=0x08), value 99
    //                   field 2 (string, tag=0x12), len 3, "Bob"
    let manual_bytes = vec![
        0x08, // field 1, wire type 0
        99,   // varint 99
        0x12, // field 2, wire type 2
        3,    // length 3
        b'B', b'o', b'b',
    ];

    let decoded = proto_binary_to_json(&manual_bytes, &fields).unwrap();
    assert_eq!(decoded["id"], 99);
    assert_eq!(decoded["name"], "Bob");
}

// ---------------------------------------------------------------------------
// Roundtrip: JSON → proto → JSON
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_json_proto_json() {
    let fields = vec![
        scalar_field("id", "uint32", 1),
        scalar_field("name", "string", 2),
        scalar_field("age", "int32", 3),
        scalar_field("verified", "bool", 4),
    ];

    let original = serde_json::json!({
        "id": 123,
        "name": "Charlie",
        "age": 30,
        "verified": true
    });

    let proto_bytes = json_to_proto_binary(&original, &fields).unwrap();
    let roundtripped = proto_binary_to_json(&proto_bytes, &fields).unwrap();

    assert_eq!(roundtripped["id"], 123);
    assert_eq!(roundtripped["name"], "Charlie");
    assert_eq!(roundtripped["age"], 30);
    assert_eq!(roundtripped["verified"], true);
}

// ---------------------------------------------------------------------------
// Unknown fields are skipped
// ---------------------------------------------------------------------------

#[test]
fn unknown_fields_skipped() {
    // Encode with 3 fields, decode with only 2 (field 2 is "unknown").
    let encode_fields = vec![
        scalar_field("a", "uint32", 1),
        scalar_field("b", "string", 2),
        scalar_field("c", "bool", 3),
    ];
    let decode_fields = vec![
        scalar_field("a", "uint32", 1),
        // field 2 is missing — should be skipped
        scalar_field("c", "bool", 3),
    ];

    let json = serde_json::json!({"a": 10, "b": "secret", "c": true});
    let bytes = json_to_proto_binary(&json, &encode_fields).unwrap();
    let decoded = proto_binary_to_json(&bytes, &decode_fields).unwrap();

    assert_eq!(decoded["a"], 10);
    assert_eq!(decoded["c"], true);
    assert!(decoded.get("b").is_none(), "unknown field 'b' should be skipped");
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn empty_bytes_decodes_to_empty_object() {
    let fields = vec![scalar_field("id", "uint32", 1)];
    let decoded = proto_binary_to_json(&[], &fields).unwrap();
    assert_eq!(decoded, serde_json::json!({}));
}

#[test]
fn empty_json_object_encodes_to_empty_bytes() {
    let fields = vec![scalar_field("id", "uint32", 1)];
    let bytes = json_to_proto_binary(&serde_json::json!({}), &fields).unwrap();
    assert!(bytes.is_empty());
}

#[test]
fn null_fields_are_omitted() {
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
fn wire_type_for_common_types() {
    assert_eq!(wire_type_for("uint32"), 0);
    assert_eq!(wire_type_for("int64"), 0);
    assert_eq!(wire_type_for("bool"), 0);
    assert_eq!(wire_type_for("sint32"), 0);
    assert_eq!(wire_type_for("double"), 1);
    assert_eq!(wire_type_for("fixed64"), 1);
    assert_eq!(wire_type_for("string"), 2);
    assert_eq!(wire_type_for("bytes"), 2);
    assert_eq!(wire_type_for("float"), 5);
    assert_eq!(wire_type_for("fixed32"), 5);
    assert_eq!(wire_type_for("UserMessage"), 2); // message types
}

// ---------------------------------------------------------------------------
// Nested messages
// ---------------------------------------------------------------------------

#[test]
fn nested_message_roundtrip() {
    let address_fields = vec![
        scalar_field("street", "string", 1),
        scalar_field("zip", "uint32", 2),
    ];

    let user_fields = vec![
        scalar_field("name", "string", 1),
        ProtoFieldDef {
            name: "address".into(),
            proto_type: "Address".into(),
            tag: 2,
            repeated: false,
            is_map: false,
            map_key_type: None,
            map_value_type: None,
            nested_fields: Some(address_fields),
        },
    ];

    let json = serde_json::json!({
        "name": "Eve",
        "address": {"street": "456 Oak Ave", "zip": 90210}
    });

    let bytes = json_to_proto_binary(&json, &user_fields).unwrap();
    let decoded = proto_binary_to_json(&bytes, &user_fields).unwrap();

    assert_eq!(decoded["name"], "Eve");
    assert_eq!(decoded["address"]["street"], "456 Oak Ave");
    assert_eq!(decoded["address"]["zip"], 90210);
}

// ---------------------------------------------------------------------------
// Repeated fields
// ---------------------------------------------------------------------------

#[test]
fn repeated_uint32_roundtrip() {
    let fields = vec![ProtoFieldDef {
        name: "values".into(),
        proto_type: "uint32".into(),
        tag: 1,
        repeated: true,
        is_map: false,
        map_key_type: None,
        map_value_type: None,
        nested_fields: None,
    }];

    let json = serde_json::json!({"values": [10, 20, 30]});
    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
    assert_eq!(decoded["values"], serde_json::json!([10, 20, 30]));
}

#[test]
fn repeated_string_roundtrip() {
    let fields = vec![ProtoFieldDef {
        name: "tags".into(),
        proto_type: "string".into(),
        tag: 1,
        repeated: true,
        is_map: false,
        map_key_type: None,
        map_value_type: None,
        nested_fields: None,
    }];

    let json = serde_json::json!({"tags": ["rust", "grpc", "web"]});
    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
    assert_eq!(decoded["tags"], serde_json::json!(["rust", "grpc", "web"]));
}

// ---------------------------------------------------------------------------
// Sint32 / Sint64 (ZigZag)
// ---------------------------------------------------------------------------

#[test]
fn sint32_roundtrip() {
    let fields = vec![scalar_field("delta", "sint32", 1)];

    for val in [-1i32, 1, -100, 100, i32::MIN, i32::MAX] {
        let json = serde_json::json!({"delta": val});
        let bytes = json_to_proto_binary(&json, &fields).unwrap();
        let decoded = proto_binary_to_json(&bytes, &fields).unwrap();
        assert_eq!(decoded["delta"], val, "sint32 roundtrip failed for {val}");
    }
    // 0 is proto3 default — omitted from wire
    let json = serde_json::json!({"delta": 0});
    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    assert!(bytes.is_empty(), "sint32 zero is proto3 default, should not be encoded");
}
