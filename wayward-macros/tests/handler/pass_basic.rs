use wayward_macros::handler;

#[handler]
async fn hello() -> &'static str {
    "hello"
}

fn main() {}
