use typeway_macros::handler;

struct NotAnExtractor;

#[handler]
async fn bad_extractor(x: NotAnExtractor, y: String) -> &'static str {
    let _ = (x, y);
    "hello"
}

fn main() {}
