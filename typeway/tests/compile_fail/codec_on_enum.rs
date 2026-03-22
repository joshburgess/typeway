// Error: #[derive(TypewayCodec)] only supports structs, not enums.

use typeway_macros::TypewayCodec;

#[derive(TypewayCodec)]
enum Status {
    Active,
    Inactive,
}

fn main() {}
