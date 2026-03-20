use typeway_grpc::ToProtoType;
use typeway_macros::ToProtoType;

#[derive(ToProtoType)]
struct User {
    #[proto(tag = 1)]
    id: u32,
    #[proto(tag = 2)]
    name: String,
}

fn main() {
    assert_eq!(User::proto_type_name(), "User");
    assert!(User::is_message());
    let def = User::message_definition().unwrap();
    assert!(def.contains("uint32 id = 1"));
    assert!(def.contains("string name = 2"));
    assert!(def.contains("message User {"));
}
