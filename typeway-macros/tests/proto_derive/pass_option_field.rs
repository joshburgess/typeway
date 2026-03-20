use typeway_grpc::ToProtoType;
use typeway_macros::ToProtoType;

#[derive(ToProtoType)]
struct Profile {
    #[proto(tag = 1)]
    name: String,
    #[proto(tag = 2)]
    bio: Option<String>,
}

fn main() {
    let def = Profile::message_definition().unwrap();
    assert!(def.contains("string name = 1;"), "got: {def}");
    assert!(def.contains("optional string bio = 2;"), "got: {def}");
}
