//! Example: using auto_grpc_client! with the RealWorld API.
//!
//! This file is not compiled as part of the binary -- it serves as a code
//! example showing how to create a type-safe gRPC client that's automatically
//! derived from the same API type as the server.
//!
//! ```ignore
//! use typeway_grpc::auto_grpc_client;
//! use crate::api::RealWorldAPI;
//!
//! auto_grpc_client! {
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
//!     // List users via gRPC (same data as GET /api/users via REST)
//!     let users = client.call_method("ListArticle", serde_json::json!({})).await.unwrap();
//!     println!("Articles: {}", users);
//!
//!     // Get tags via gRPC
//!     let tags = client.call_method("ListTag", serde_json::json!({})).await.unwrap();
//!     println!("Tags: {}", tags);
//!
//!     // Get health check
//!     let health = client.call_method("GetHealth", serde_json::json!({})).await.unwrap();
//!     println!("Health: {}", health);
//!
//!     // Get site stats
//!     let stats = client.call_method("GetStats", serde_json::json!({})).await.unwrap();
//!     println!("Stats: {}", stats);
//!
//!     // The .proto file for this service:
//!     println!("{}", client.proto());
//! }
//! ```
//!
//! # Native gRPC client (Phase 4)
//!
//! The native client supports codec selection (JSON or binary protobuf)
//! and the same interceptor/config system:
//!
//! ```ignore
//! use typeway_grpc::native_grpc_client;
//! use crate::api::RealWorldAPI;
//!
//! native_grpc_client! {
//!     pub struct RealWorldNativeClient;
//!     api = RealWorldAPI;
//!     service = "RealWorldService";
//!     package = "realworld.v1";
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     // JSON codec (default, compatible with typeway bridge servers)
//!     let client = RealWorldNativeClient::new("http://localhost:4000").unwrap();
//!
//!     // Or with binary protobuf codec (for standard gRPC servers)
//!     // let codec = Arc::new(typeway_grpc::codec::BinaryCodec::new(transcoder));
//!     // let client = RealWorldNativeClient::with_codec("http://localhost:4000", codec).unwrap();
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
//!     let desc = client.service_descriptor();
//!     println!("Methods: {}", desc.methods.len());
//!     println!("{}", client.proto());
//! }
//! ```
