// Error: handler must be async.
use wayward::prelude::*;

#[handler]
fn not_async() -> &'static str {
    "hello"
}

fn main() {}
