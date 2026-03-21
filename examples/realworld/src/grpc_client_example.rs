//! Example: using `grpc_client!` with the RealWorld API.
//!
//! This file is not compiled as part of the binary — it serves as a code
//! example showing how to create a type-safe gRPC client derived from the
//! same API type as the server.
//!
//! ```ignore
//! use typeway_grpc::grpc_client;
//! use crate::api::RealWorldAPI;
//!
//! grpc_client! {
//!     pub struct RealWorldGrpcClient;
//!     api = RealWorldAPI;
//!     service = "RealWorldService";
//!     package = "realworld.v1";
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let client = RealWorldGrpcClient::new("http://localhost:4000").unwrap();
//!
//!     // Discover available methods:
//!     let desc = client.service_descriptor();
//!     for method in &desc.methods {
//!         println!("{} {} -> {}", method.http_method, method.rest_path, method.name);
//!     }
//!
//!     // Unary call
//!     let articles = client.call("ListArticle", &serde_json::json!({})).await.unwrap();
//!     println!("Articles: {articles}");
//!
//!     // Server-streaming call
//!     let mut stream = client
//!         .call_server_stream("ListArticle", &serde_json::json!({}))
//!         .await
//!         .unwrap();
//!     while let Some(item) = stream.recv().await {
//!         println!("Item: {}", item.unwrap());
//!     }
//!
//!     // Service discovery & proto generation
//!     println!("Methods: {}", desc.methods.len());
//!     println!("{}", client.proto());
//! }
//! ```
