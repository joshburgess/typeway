// TypewayCodec on a simple (fieldless) enum compiles — encodes as varint.

use typeway_protobuf::{TypewayEncode, TypewayDecode};
use typeway_macros::TypewayCodec;

#[derive(TypewayCodec, Debug, PartialEq)]
enum Status {
    #[proto(tag = 0)]
    Active,
    #[proto(tag = 1)]
    Inactive,
    #[proto(tag = 2)]
    Suspended,
}

#[derive(TypewayCodec, Debug, PartialEq)]
enum Side {
    Buy,
    Sell,
}

fn _check() {
    fn _assert_encode<T: TypewayEncode>() {}
    fn _assert_decode<T: TypewayDecode>() {}
    _assert_encode::<Status>();
    _assert_decode::<Status>();
    _assert_encode::<Side>();
    _assert_decode::<Side>();
}

fn main() {}
