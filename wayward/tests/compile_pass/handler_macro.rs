// The #[handler] macro validates correct handlers.
use wayward::prelude::*;

#[handler]
async fn no_args() -> &'static str {
    "ok"
}

#[handler]
async fn returns_string() -> String {
    "ok".to_string()
}

#[handler]
async fn returns_status() -> http::StatusCode {
    http::StatusCode::OK
}

#[handler]
async fn returns_tuple() -> (http::StatusCode, &'static str) {
    (http::StatusCode::CREATED, "done")
}

#[handler]
async fn returns_json() -> Json<Vec<u32>> {
    Json(vec![1, 2, 3])
}

fn main() {}
