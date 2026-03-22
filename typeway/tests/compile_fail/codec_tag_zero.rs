// Error: proto tag 0 is reserved and rejected at compile time.

use typeway_macros::TypewayCodec;

#[derive(TypewayCodec)]
struct BadMessage {
    #[proto(tag = 0)]
    name: String,
}

fn main() {}
