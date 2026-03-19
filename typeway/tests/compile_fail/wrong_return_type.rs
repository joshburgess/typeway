// Error: handler return type doesn't implement IntoResponse.
use typeway::prelude::*;

typeway_path!(type HelloPath = "hello");

struct NotAResponse;

#[handler]
async fn bad_handler() -> NotAResponse {
    NotAResponse
}

fn main() {}
