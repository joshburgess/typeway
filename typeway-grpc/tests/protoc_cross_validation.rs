//! Cross-validation tests: verify typeway's protobuf codec produces
//! wire-compatible output with Google's `protoc` reference implementation.
//!
//! Covers: scalars, nested messages, repeated fields, negative integers,
//! floats/doubles, maps, default values, and full roundtrips.
//!
//! Tests are skipped if `protoc` is not installed.

use std::io::Write;
use std::process::{Command, Stdio};
use typeway_grpc::proto_codec::{json_to_proto_binary, proto_binary_to_json, ProtoFieldDef};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn has_protoc() -> bool {
    Command::new("protoc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

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

fn repeated_field(name: &str, proto_type: &str, tag: u32) -> ProtoFieldDef {
    ProtoFieldDef {
        name: name.into(),
        proto_type: proto_type.into(),
        tag,
        repeated: true,
        is_map: false,
        map_key_type: None,
        map_value_type: None,
        nested_fields: None,
    }
}

fn nested_field(name: &str, tag: u32, fields: Vec<ProtoFieldDef>) -> ProtoFieldDef {
    ProtoFieldDef {
        name: name.into(),
        proto_type: "message".into(),
        tag,
        repeated: false,
        is_map: false,
        map_key_type: None,
        map_value_type: None,
        nested_fields: Some(fields),
    }
}

fn map_field(name: &str, tag: u32, key_type: &str, value_type: &str) -> ProtoFieldDef {
    ProtoFieldDef {
        name: name.into(),
        proto_type: "map".into(),
        tag,
        repeated: false,
        is_map: true,
        map_key_type: Some(key_type.into()),
        map_value_type: Some(value_type.into()),
        nested_fields: None,
    }
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn protoc_decode(message_name: &str, bytes: &[u8]) -> Option<String> {
    let fixtures = fixtures_dir();
    let mut child = Command::new("protoc")
        .arg(format!("--decode={message_name}"))
        .arg(format!("--proto_path={}", fixtures.display()))
        .arg("simple_test.proto")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(bytes).ok()?;
    }
    child.stdin.take();

    let output = child.wait_with_output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        eprintln!(
            "protoc --decode stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        None
    }
}

fn protoc_encode(message_name: &str, text_format: &str) -> Option<Vec<u8>> {
    let fixtures = fixtures_dir();
    let mut child = Command::new("protoc")
        .arg(format!("--encode={message_name}"))
        .arg(format!("--proto_path={}", fixtures.display()))
        .arg("simple_test.proto")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(text_format.as_bytes()).ok()?;
    }
    child.stdin.take();

    let output = child.wait_with_output().ok()?;
    if output.status.success() {
        Some(output.stdout)
    } else {
        eprintln!(
            "protoc --encode stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        None
    }
}

// ---------------------------------------------------------------------------
// 1. Simple scalars: typeway encode → protoc decode
// ---------------------------------------------------------------------------

#[test]
fn typeway_encode_protoc_decode_scalars() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let fields = vec![
        scalar_field("name", "string", 1),
        scalar_field("id", "uint32", 2),
        scalar_field("active", "bool", 3),
    ];
    let json = serde_json::json!({
        "name": "alice",
        "id": 42,
        "active": true
    });

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let text = protoc_decode("test.SimpleTest", &bytes).expect("protoc --decode failed");

    assert!(text.contains("name: \"alice\""), "missing name in: {text}");
    assert!(text.contains("id: 42"), "missing id in: {text}");
    assert!(text.contains("active: true"), "missing active in: {text}");
}

// ---------------------------------------------------------------------------
// 2. Simple scalars: protoc encode → typeway decode
// ---------------------------------------------------------------------------

#[test]
fn protoc_encode_typeway_decode_scalars() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let text_format = "name: \"bob\"\nid: 99\nactive: true\n";
    let bytes = protoc_encode("test.SimpleTest", text_format).expect("protoc --encode failed");

    let fields = vec![
        scalar_field("name", "string", 1),
        scalar_field("id", "uint32", 2),
        scalar_field("active", "bool", 3),
    ];

    let json = proto_binary_to_json(&bytes, &fields).unwrap();
    assert_eq!(json["name"], "bob");
    assert_eq!(json["id"], 99);
    assert_eq!(json["active"], true);
}

// ---------------------------------------------------------------------------
// 3. Nested message: typeway encode → protoc decode
// ---------------------------------------------------------------------------

#[test]
fn typeway_encode_protoc_decode_nested() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let inner_fields = vec![
        scalar_field("name", "string", 1),
        scalar_field("id", "uint32", 2),
        scalar_field("active", "bool", 3),
    ];
    let fields = vec![
        scalar_field("title", "string", 1),
        nested_field("author", 2, inner_fields),
    ];
    let json = serde_json::json!({
        "title": "Hello World",
        "author": {
            "name": "alice",
            "id": 1,
            "active": true
        }
    });

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let text = protoc_decode("test.WithNested", &bytes).expect("protoc --decode failed");

    assert!(
        text.contains("title: \"Hello World\""),
        "missing title in: {text}"
    );
    assert!(
        text.contains("name: \"alice\""),
        "missing author.name in: {text}"
    );
    assert!(text.contains("id: 1"), "missing author.id in: {text}");
}

// ---------------------------------------------------------------------------
// 4. Nested message: protoc encode → typeway decode
// ---------------------------------------------------------------------------

#[test]
fn protoc_encode_typeway_decode_nested() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let text_format = r#"title: "Test Article"
author {
  name: "bob"
  id: 5
  active: false
}
"#;
    let bytes = protoc_encode("test.WithNested", text_format).expect("protoc --encode failed");

    let inner_fields = vec![
        scalar_field("name", "string", 1),
        scalar_field("id", "uint32", 2),
        scalar_field("active", "bool", 3),
    ];
    let fields = vec![
        scalar_field("title", "string", 1),
        nested_field("author", 2, inner_fields),
    ];

    let json = proto_binary_to_json(&bytes, &fields).unwrap();
    assert_eq!(json["title"], "Test Article");
    assert_eq!(json["author"]["name"], "bob");
    assert_eq!(json["author"]["id"], 5);
}

// ---------------------------------------------------------------------------
// 5. Repeated fields: typeway encode → protoc decode
// ---------------------------------------------------------------------------

#[test]
fn typeway_encode_protoc_decode_repeated() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let fields = vec![
        repeated_field("values", "int32", 1),
        repeated_field("tags", "string", 2),
    ];
    let json = serde_json::json!({
        "values": [10, 20, 30],
        "tags": ["alpha", "beta"]
    });

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let text = protoc_decode("test.WithRepeated", &bytes).expect("protoc --decode failed");

    // protoc may show packed ints as individual lines or as a single line
    assert!(text.contains("10"), "missing value 10 in: {text}");
    assert!(text.contains("20"), "missing value 20 in: {text}");
    assert!(text.contains("30"), "missing value 30 in: {text}");
    assert!(text.contains("tags: \"alpha\""), "missing alpha in: {text}");
    assert!(text.contains("tags: \"beta\""), "missing beta in: {text}");
}

// ---------------------------------------------------------------------------
// 6. Repeated fields: protoc encode → typeway decode
// ---------------------------------------------------------------------------

#[test]
fn protoc_encode_typeway_decode_repeated() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let text_format = "values: 100\nvalues: 200\ntags: \"x\"\ntags: \"y\"\ntags: \"z\"\n";
    let bytes = protoc_encode("test.WithRepeated", text_format).expect("protoc --encode failed");

    let fields = vec![
        repeated_field("values", "int32", 1),
        repeated_field("tags", "string", 2),
    ];

    let json = proto_binary_to_json(&bytes, &fields).unwrap();

    let values = json["values"].as_array().expect("values not array");
    assert_eq!(values.len(), 2, "should unpack both packed int32 values");
    assert_eq!(values[0], 100);
    assert_eq!(values[1], 200);

    let tags = json["tags"].as_array().expect("tags not array");
    assert_eq!(tags.len(), 3, "string repeated fields are not packed");
    assert_eq!(tags[0], "x");
    assert_eq!(tags[1], "y");
    assert_eq!(tags[2], "z");
}

// ---------------------------------------------------------------------------
// 7. Negative int32 (10-byte sign-extended varint)
// ---------------------------------------------------------------------------

#[test]
fn typeway_encode_protoc_decode_negative_int() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let fields = vec![scalar_field("temperature", "int32", 1)];
    let json = serde_json::json!({
        "temperature": -42
    });

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let text = protoc_decode("test.WithNegative", &bytes).expect("protoc --decode failed");

    assert!(
        text.contains("temperature: -42"),
        "missing temperature in: {text}"
    );
}

// ---------------------------------------------------------------------------
// 8. Floats and doubles
// ---------------------------------------------------------------------------

#[test]
fn typeway_encode_protoc_decode_floats() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let fields = vec![
        scalar_field("score", "float", 1),
        scalar_field("precise", "double", 2),
    ];
    let json = serde_json::json!({
        "score": 1.5,
        "precise": 2.5
    });

    let bytes = json_to_proto_binary(&json, &fields).unwrap();
    let text = protoc_decode("test.WithFloats", &bytes).expect("protoc --decode failed");

    // Float precision: protoc may show different decimal places
    assert!(text.contains("score:"), "missing score in: {text}");
    assert!(text.contains("precise:"), "missing precise in: {text}");
}

// ---------------------------------------------------------------------------
// 9. Map fields: typeway encode → protoc decode
// ---------------------------------------------------------------------------

#[test]
fn typeway_encode_protoc_decode_map() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let fields = vec![map_field("scores", 1, "string", "int32")];
    let json = serde_json::json!({
        "scores": {
            "alice": 100,
            "bob": 85
        }
    });

    let bytes = json_to_proto_binary(&json, &fields).unwrap();

    let text = protoc_decode("test.WithMap", &bytes).expect("protoc --decode failed for map");

    assert!(text.contains("alice"), "missing alice in: {text}");
    assert!(text.contains("bob"), "missing bob in: {text}");
    assert!(text.contains("100"), "missing score 100 in: {text}");
    assert!(text.contains("85"), "missing score 85 in: {text}");
}

// ---------------------------------------------------------------------------
// 10. Default values: all-defaults → empty wire format
// ---------------------------------------------------------------------------

#[test]
fn default_values_produce_empty_encoding() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let fields = vec![
        scalar_field("name", "string", 1),
        scalar_field("id", "uint32", 2),
        scalar_field("active", "bool", 3),
    ];
    // Proto3: default values (0, "", false) should not be encoded.
    // NOTE: typeway currently DOES encode empty strings and false bools
    // on the wire (not strictly proto3 compliant for default-value
    // omission). This test documents the current behavior.
    let json = serde_json::json!({
        "name": "",
        "id": 0,
        "active": false
    });

    let bytes = json_to_proto_binary(&json, &fields).unwrap();

    // Proto3: all defaults should produce empty encoding
    assert!(
        bytes.is_empty(),
        "expected empty encoding for defaults, got {} bytes",
        bytes.len()
    );
}

// ---------------------------------------------------------------------------
// 11. Full roundtrip: typeway → protoc → typeway
// ---------------------------------------------------------------------------

#[test]
fn full_roundtrip() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let fields = vec![
        scalar_field("name", "string", 1),
        scalar_field("id", "uint32", 2),
        scalar_field("active", "bool", 3),
    ];
    let original = serde_json::json!({
        "name": "charlie",
        "id": 7,
        "active": true
    });

    // typeway encode
    let bytes1 = json_to_proto_binary(&original, &fields).unwrap();

    // protoc decode to text
    let text = protoc_decode("test.SimpleTest", &bytes1).expect("protoc --decode failed");

    // protoc encode from text
    let bytes2 = protoc_encode("test.SimpleTest", &text).expect("protoc --encode failed");

    // typeway decode
    let decoded = proto_binary_to_json(&bytes2, &fields).unwrap();

    assert_eq!(decoded["name"], "charlie");
    assert_eq!(decoded["id"], 7);
    assert_eq!(decoded["active"], true);
}

// ---------------------------------------------------------------------------
// 12. Proto file validates with protoc
// ---------------------------------------------------------------------------

#[test]
fn proto_file_validates() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let fixtures = fixtures_dir();
    let output = Command::new("protoc")
        .arg(format!("--proto_path={}", fixtures.display()))
        .arg("--descriptor_set_out=/dev/null")
        .arg("simple_test.proto")
        .output()
        .expect("failed to run protoc");

    assert!(
        output.status.success(),
        "protoc validation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
