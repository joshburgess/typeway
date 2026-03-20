use typeway_grpc::ToProtoType;
use typeway_macros::ToProtoType;

#[derive(ToProtoType)]
enum Payload {
    Text(String),
    Binary(Vec<u8>),
}

fn main() {
    assert_eq!(Payload::proto_type_name(), "Payload");
    assert!(Payload::is_message());
    let def = Payload::message_definition().unwrap();
    assert!(def.contains("message Payload {"), "got: {def}");
    assert!(def.contains("oneof payload {"), "got: {def}");
    assert!(def.contains("string text = 1"), "got: {def}");
    assert!(def.contains("bytes binary = 2"), "got: {def}");
}
