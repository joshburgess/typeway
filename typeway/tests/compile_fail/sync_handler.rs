// Error: handler must be async.
use typeway::prelude::*;

#[handler]
fn not_async() -> &'static str {
    "hello"
}

fn main() {}
