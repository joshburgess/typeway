// #[derive(TypewayCodec)] compiles for structs with various field types.

use typeway_protobuf::{BytesStr, TypewayEncode, TypewayDecode, ProtoMessage};
use typeway_macros::TypewayCodec;

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default)]
struct Simple {
    #[proto(tag = 1)]
    name: String,
    #[proto(tag = 2)]
    age: u32,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default)]
struct AllTypes {
    #[proto(tag = 1)]
    str_field: String,
    #[proto(tag = 2)]
    bytes_str_field: BytesStr,
    #[proto(tag = 3)]
    u32_field: u32,
    #[proto(tag = 4)]
    u64_field: u64,
    #[proto(tag = 5)]
    i32_field: i32,
    #[proto(tag = 6)]
    i64_field: i64,
    #[proto(tag = 7)]
    f32_field: f32,
    #[proto(tag = 8)]
    f64_field: f64,
    #[proto(tag = 9)]
    bool_field: bool,
    #[proto(tag = 10)]
    bytes_field: Vec<u8>,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default)]
struct WithOptional {
    #[proto(tag = 1)]
    required: String,
    #[proto(tag = 2)]
    optional: Option<String>,
    #[proto(tag = 3)]
    optional_num: Option<u32>,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default)]
struct WithRepeated {
    #[proto(tag = 1)]
    tags: Vec<String>,
    #[proto(tag = 2)]
    scores: Vec<u32>,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default)]
struct Nested {
    #[proto(tag = 1)]
    inner: Simple,
    #[proto(tag = 2)]
    items: Vec<Simple>,
}

fn _check() {
    fn _assert_encode<T: TypewayEncode>() {}
    fn _assert_decode<T: TypewayDecode>() {}
    fn _assert_proto_message<T: ProtoMessage>() {}

    _assert_encode::<Simple>();
    _assert_decode::<Simple>();
    _assert_proto_message::<Simple>();

    _assert_encode::<AllTypes>();
    _assert_decode::<AllTypes>();

    _assert_encode::<WithOptional>();
    _assert_decode::<WithOptional>();

    _assert_encode::<WithRepeated>();
    _assert_decode::<WithRepeated>();

    _assert_encode::<Nested>();
    _assert_decode::<Nested>();
}

fn main() {}
