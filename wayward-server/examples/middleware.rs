//! Demonstrates Tower middleware layers with wayward.
//!
//! Run: cargo run -p wayward-server --example middleware
//! Test: curl -v http://127.0.0.1:3000/hello

use std::time::Duration;

use wayward_core::*;
use wayward_macros::*;
use wayward_server::*;

use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;

wayward_path!(type HelloPath = "hello");
wayward_path!(type SlowPath = "slow");

type API = (
    GetEndpoint<HelloPath, String>,
    GetEndpoint<SlowPath, String>,
);

async fn hello() -> &'static str {
    "Hello with middleware!"
}

async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_secs(5)).await;
    "This was slow"
}

#[tokio::main]
async fn main() {
    let server = Server::<API>::new((bind::<_, _, _>(hello), bind::<_, _, _>(slow)));

    println!("Wayward middleware example on http://127.0.0.1:3000");
    println!("  GET /hello - fast response");
    println!("  GET /slow  - 5s delay (will timeout after 2s)");
    println!();
    println!("Middleware stack:");
    println!("  - CorsLayer (permissive)");
    println!("  - TimeoutLayer (2s)");

    server
        .layer(CorsLayer::permissive())
        .layer(TimeoutLayer::with_status_code(
            http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(2),
        ))
        .serve("127.0.0.1:3000".parse().unwrap())
        .await
        .unwrap();
}
