// Error: #[derive(TypewayCodec)] on a unit struct (no fields).

use typeway_macros::TypewayCodec;

#[derive(TypewayCodec)]
struct Empty;

fn main() {}
