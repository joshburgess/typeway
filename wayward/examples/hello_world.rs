//! Minimal wayward server example.
//!
//! Run: cargo run -p wayward --example hello
//! Test: curl http://127.0.0.1:3000/hello

use wayward::prelude::*;

wayward_path!(type HelloPath = "hello");

type API = (GetEndpoint<HelloPath, String>,);

async fn hello() -> &'static str {
    "Hello from Wayward!"
}

#[tokio::main]
async fn main() {
    Server::<API>::new((bind::<_, _, _>(hello),))
        .serve("127.0.0.1:3000".parse().unwrap())
        .await
        .unwrap();
}
