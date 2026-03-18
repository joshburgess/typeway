use wayward_macros::handler;

struct NotAResponse;

#[handler]
async fn bad_return() -> NotAResponse {
    NotAResponse
}

fn main() {}
