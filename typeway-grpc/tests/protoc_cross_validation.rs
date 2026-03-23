//! Cross-validation tests: verify typeway's protobuf codec produces
//! wire-compatible output with Google's `protoc` reference implementation.
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
// typeway encode → protoc decode
// ---------------------------------------------------------------------------

#[test]
fn typeway_encode_protoc_decode_string() {
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

    assert!(
        text.contains("name: \"alice\""),
        "missing name in: {text}"
    );
    assert!(text.contains("id: 42"), "missing id in: {text}");
    assert!(text.contains("active: true"), "missing active in: {text}");
}

// ---------------------------------------------------------------------------
// protoc encode → typeway decode
// ---------------------------------------------------------------------------

#[test]
fn protoc_encode_typeway_decode() {
    if !has_protoc() {
        eprintln!("SKIP: protoc not found");
        return;
    }

    let text_format = "name: \"bob\"\nid: 99\nactive: false\n";
    let bytes = protoc_encode("test.SimpleTest", text_format).expect("protoc --encode failed");

    let fields = vec![
        scalar_field("name", "string", 1),
        scalar_field("id", "uint32", 2),
        scalar_field("active", "bool", 3),
    ];

    let json = proto_binary_to_json(&bytes, &fields).unwrap();
    assert_eq!(json["name"], "bob");
    assert_eq!(json["id"], 99);
    // proto3: false is default, may be absent in JSON
}

// ---------------------------------------------------------------------------
// Full roundtrip: typeway → protoc → typeway
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
// Proto file validates with protoc
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
