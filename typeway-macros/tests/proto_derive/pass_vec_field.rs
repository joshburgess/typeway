use typeway_grpc::ToProtoType;
use typeway_macros::ToProtoType;

#[derive(ToProtoType)]
struct TagList {
    #[proto(tag = 1)]
    tags: Vec<String>,
    #[proto(tag = 2)]
    data: Vec<u8>,
}

fn main() {
    let def = TagList::message_definition().unwrap();
    // Vec<String> should produce "repeated string"
    assert!(def.contains("repeated string tags = 1;"), "got: {def}");
    // Vec<u8> should produce "bytes" (not repeated)
    assert!(def.contains("bytes data = 2;"), "got: {def}");
    assert!(!def.contains("repeated bytes"), "Vec<u8> should not be repeated, got: {def}");
}
