// Error: #[derive(TypewayCodec)] only supports structs with named fields.

use typeway_macros::TypewayCodec;

#[derive(TypewayCodec)]
struct Wrapper(u32, String);

fn main() {}
