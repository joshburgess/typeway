// #[derive(TypewayCodec)] with BytesStr fields compiles and satisfies ProtoMessage.

use typeway_protobuf::{BytesStr, TypewayEncode, TypewayDecode, ProtoMessage};
use typeway_macros::TypewayCodec;

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default)]
struct Order {
    #[proto(tag = 1)]
    symbol: BytesStr,
    #[proto(tag = 2)]
    side: BytesStr,
    #[proto(tag = 3)]
    price: f64,
    #[proto(tag = 4)]
    quantity: u32,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default)]
struct OrderWithOptional {
    #[proto(tag = 1)]
    symbol: BytesStr,
    #[proto(tag = 2)]
    notes: Option<BytesStr>,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default)]
struct OrderWithRepeated {
    #[proto(tag = 1)]
    symbol: BytesStr,
    #[proto(tag = 2)]
    tags: Vec<BytesStr>,
}

fn _check() {
    fn _assert_proto_message<T: ProtoMessage>() {}
    _assert_proto_message::<Order>();
    _assert_proto_message::<OrderWithOptional>();
    _assert_proto_message::<OrderWithRepeated>();

    // BytesStr itself satisfies encode/decode.
    fn _assert_encode<T: TypewayEncode>() {}
    fn _assert_decode<T: TypewayDecode>() {}
    _assert_encode::<BytesStr>();
    _assert_decode::<BytesStr>();
}

fn main() {}
