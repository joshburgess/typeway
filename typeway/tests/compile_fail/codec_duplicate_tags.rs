// Error: duplicate proto tags are rejected at compile time.

use typeway_macros::TypewayCodec;

#[derive(TypewayCodec)]
struct BadMessage {
    #[proto(tag = 1)]
    name: String,
    #[proto(tag = 1)]
    other: String,
}

fn main() {}
