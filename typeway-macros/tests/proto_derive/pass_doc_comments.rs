use typeway_grpc::ToProtoType;
use typeway_macros::ToProtoType;

#[derive(ToProtoType)]
struct User {
    /// The unique ID.
    #[proto(tag = 1)]
    id: u32,
    /// Display name.
    #[proto(tag = 2)]
    name: String,
}

fn main() {
    let def = User::message_definition().unwrap();
    assert!(def.contains("// The unique ID."), "got: {def}");
    assert!(def.contains("// Display name."), "got: {def}");
    assert!(def.contains("uint32 id = 1"), "got: {def}");
    assert!(def.contains("string name = 2"), "got: {def}");
}
