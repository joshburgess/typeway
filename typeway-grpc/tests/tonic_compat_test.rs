#![cfg(feature = "tonic-compat")]

use typeway_grpc::mapping::ToProtoType;
use typeway_grpc::tonic_compat::{base64_decode, base64_encode, Protobuf, ProtobufError};

#[test]
fn base64_roundtrip() {
    let data = b"Hello, protobuf!";
    let encoded = base64_encode(data);
    let decoded = base64_decode(&encoded).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn base64_roundtrip_binary_data() {
    let data: Vec<u8> = (0..=255).collect();
    let encoded = base64_encode(&data);
    let decoded = base64_decode(&encoded).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn protobuf_error_display() {
    let err = ProtobufError::Decode("bad data".into());
    assert!(err.to_string().contains("bad data"));

    let err = ProtobufError::Encode("buffer full".into());
    assert!(err.to_string().contains("buffer full"));
}

#[test]
fn impl_proto_type_macro_compiles() {
    struct FakeMessage;
    typeway_grpc::impl_proto_type_for_prost!(FakeMessage);
    assert_eq!(FakeMessage::proto_type_name(), "FakeMessage");
    assert!(FakeMessage::is_message());
    assert!(FakeMessage::message_definition().is_none());
}

#[test]
fn impl_proto_type_macro_with_custom_name() {
    struct FakeMsg;
    typeway_grpc::impl_proto_type_for_prost!(FakeMsg, "custom.FakeMsg");
    assert_eq!(FakeMsg::proto_type_name(), "custom.FakeMsg");
    assert!(FakeMsg::is_message());
}

#[test]
fn protobuf_wrapper_debug_and_clone() {
    let p = Protobuf(42u32);
    let debug = format!("{p:?}");
    assert!(debug.contains("Protobuf"));
    assert!(debug.contains("42"));

    let p2 = p.clone();
    assert_eq!(p.0, p2.0);
}
