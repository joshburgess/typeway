//! Demonstrates Tower middleware layers with typeway.
//!
//! Run: cargo run -p typeway-server --example middleware
//! Test: curl -v http://127.0.0.1:3000/hello

use std::time::Duration;

use typeway_core::*;
use typeway_macros::*;
use typeway_server::*;

use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;

typeway_path!(type HelloPath = "hello");
typeway_path!(type SlowPath = "slow");

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

    println!("Typeway middleware example on http://127.0.0.1:3000");
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
