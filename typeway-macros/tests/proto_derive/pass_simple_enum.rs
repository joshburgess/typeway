use typeway_grpc::ToProtoType;
use typeway_macros::ToProtoType;

#[derive(ToProtoType)]
enum Status {
    Active,
    Inactive,
    Banned,
}

fn main() {
    assert_eq!(Status::proto_type_name(), "Status");
    assert!(Status::is_message());
    let def = Status::message_definition().unwrap();
    assert!(def.contains("enum Status {"), "got: {def}");
    assert!(def.contains("ACTIVE = 0"), "got: {def}");
    assert!(def.contains("INACTIVE = 1"), "got: {def}");
    assert!(def.contains("BANNED = 2"), "got: {def}");
}
