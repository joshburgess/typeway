// Error: argument type doesn't implement FromRequestParts.
use typeway::prelude::*;

struct BadType;

#[handler]
async fn bad(x: BadType, y: String) -> &'static str {
    let _ = (x, y);
    "hello"
}

fn main() {}
