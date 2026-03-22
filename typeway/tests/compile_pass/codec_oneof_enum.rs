// TypewayCodec on a tagged enum compiles — encodes as protobuf oneof.

use typeway_protobuf::{TypewayEncode, TypewayDecode};
use typeway_macros::TypewayCodec;

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default, Debug, PartialEq, Clone)]
struct Inner {
    #[proto(tag = 1)]
    value: String,
}

#[derive(TypewayCodec, Debug)]
enum Value {
    #[proto(tag = 1)]
    Text(String),
    #[proto(tag = 2)]
    Number(u32),
    #[proto(tag = 3)]
    Flag(bool),
    #[proto(tag = 4)]
    Nested(Inner),
}

fn _check() {
    fn _assert_encode<T: TypewayEncode>() {}
    fn _assert_decode<T: TypewayDecode>() {}
    _assert_encode::<Value>();
    _assert_decode::<Value>();
}

fn main() {}
