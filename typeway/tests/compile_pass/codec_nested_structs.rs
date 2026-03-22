// TypewayCodec with deeply nested structs and all container types.

use typeway_protobuf::{BytesStr, TypewayEncode, TypewayDecode, ProtoMessage};
use typeway_macros::TypewayCodec;

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default)]
struct Address {
    #[proto(tag = 1)]
    street: String,
    #[proto(tag = 2)]
    city: String,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default)]
struct User {
    #[proto(tag = 1)]
    name: String,
    #[proto(tag = 2)]
    address: Address,
    #[proto(tag = 3)]
    backup_address: Option<Address>,
    #[proto(tag = 4)]
    previous_addresses: Vec<Address>,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default)]
struct Organization {
    #[proto(tag = 1)]
    name: BytesStr,
    #[proto(tag = 2)]
    members: Vec<User>,
    #[proto(tag = 3)]
    headquarters: Address,
}

fn _check() {
    fn _assert_proto_message<T: ProtoMessage>() {}
    _assert_proto_message::<Address>();
    _assert_proto_message::<User>();
    _assert_proto_message::<Organization>();
}

fn main() {}
