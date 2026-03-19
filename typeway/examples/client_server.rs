//! Demonstrates using the same API type for both server and client.
//!
//! Run: cargo run -p typeway --features full --example client_server

use std::sync::Arc;

use typeway::prelude::*;

// ---- Shared API definition (used by both server and client) ----

typeway_path!(type HelloPath = "hello");
typeway_path!(type AddPath = "add" / u32 / u32);

// Response types are what the client deserializes — the JSON payload type.
type API = (GetEndpoint<HelloPath, String>, GetEndpoint<AddPath, String>);

// Handlers return Json<T> (sets content-type), but the endpoint's Res is T
// (what the client gets after JSON deserialization).
async fn hello() -> Json<String> {
    Json("Hello from Wayward!".to_string())
}

async fn add(path: Path<AddPath>) -> Json<String> {
    let (a, b) = path.0;
    Json(format!("{a} + {b} = {}", a + b))
}

#[tokio::main]
async fn main() {
    // Start server in background.
    let server = Server::<API>::new((bind::<_, _, _>(hello), bind::<_, _, _>(add)));

    let router = Arc::new(server.into_router());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn({
        let router = router.clone();
        async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let io = hyper_util::rt::TokioIo::new(stream);
                let router = router.clone();
                tokio::spawn(async move {
                    let svc = hyper::service::service_fn(move |req| {
                        let router = router.clone();
                        async move { Ok::<_, std::convert::Infallible>(router.route(req).await) }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, svc)
                        .await;
                });
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // ---- Use the type-safe client ----

    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    // Same endpoint types as the server — if the API changes, both break.
    type HelloEndpoint = GetEndpoint<HelloPath, String>;
    type AddEndpoint = GetEndpoint<AddPath, String>;

    let greeting = client.call::<HelloEndpoint>(()).await.unwrap();
    println!("GET /hello => {greeting}");

    let sum = client.call::<AddEndpoint>((3u32, 7u32)).await.unwrap();
    println!("GET /add/3/7 => {sum}");

    println!("\nBoth calls were fully type-checked at compile time!");
}
