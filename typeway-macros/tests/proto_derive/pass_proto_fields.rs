use typeway_grpc::ToProtoType;
use typeway_macros::ToProtoType;

#[derive(ToProtoType)]
struct CreateUser {
    #[proto(tag = 1)]
    name: String,
    #[proto(tag = 2)]
    email: String,
}

fn main() {
    let fields = CreateUser::proto_fields();
    assert_eq!(fields.len(), 2, "expected 2 fields, got {}", fields.len());
    assert_eq!(fields[0].name, "name");
    assert_eq!(fields[0].proto_type, "string");
    assert_eq!(fields[0].tag, 1);
    assert_eq!(fields[1].name, "email");
    assert_eq!(fields[1].proto_type, "string");
    assert_eq!(fields[1].tag, 2);
}
