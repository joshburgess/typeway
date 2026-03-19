use typeway_macros::handler;

#[handler]
fn not_async() -> &'static str {
    "hello"
}

fn main() {}
