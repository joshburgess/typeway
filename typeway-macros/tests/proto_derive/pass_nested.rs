use typeway_grpc::ToProtoType;
use typeway_macros::ToProtoType;

#[derive(ToProtoType)]
struct Address {
    #[proto(tag = 1)]
    street: String,
    #[proto(tag = 2)]
    city: String,
}

#[derive(ToProtoType)]
struct Person {
    #[proto(tag = 1)]
    name: String,
    #[proto(tag = 2)]
    address: Address,
}

fn main() {
    // Address should be a message type.
    assert!(Address::is_message());
    assert_eq!(Address::proto_type_name(), "Address");

    // Person should be a message type with an Address field.
    assert!(Person::is_message());
    let def = Person::message_definition().unwrap();
    assert!(def.contains("string name = 1;"), "got: {def}");
    assert!(def.contains("Address address = 2;"), "got: {def}");

    // collect_messages should include both Address and Person definitions.
    let msgs = Person::collect_messages();
    assert!(msgs.len() >= 2, "expected at least 2 messages, got {}", msgs.len());
    let joined = msgs.join("\n");
    assert!(joined.contains("message Address {"), "missing Address in: {joined}");
    assert!(joined.contains("message Person {"), "missing Person in: {joined}");
}
