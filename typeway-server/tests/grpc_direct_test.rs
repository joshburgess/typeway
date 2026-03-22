//! Tests for direct gRPC handler dispatch.

#![cfg(feature = "protobuf")]

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use typeway_core::endpoint::PostEndpoint;
use typeway_core::path::{HCons, HNil, Lit, LitSegment};
use typeway_grpc::mapping::ToProtoType;
use typeway_macros::TypewayCodec;
use typeway_protobuf::{TypewayDecode, TypewayEncode};
use typeway_server::grpc_direct::into_direct_handler;
use typeway_server::*;

// --- Types ---

#[allow(non_camel_case_types)]
struct __lit_items;
impl LitSegment for __lit_items {
    const VALUE: &'static str = "items";
}
type ItemsPath = HCons<Lit<__lit_items>, HNil>;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, PartialEq)]
struct CreateItem {
    #[proto(tag = 1)]
    name: String,
    #[proto(tag = 2)]
    quantity: u32,
}

impl ToProtoType for CreateItem {
    fn proto_type_name() -> &'static str { "CreateItem" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message CreateItem {\n  string name = 1;\n  uint32 quantity = 2;\n}".to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, PartialEq)]
struct Item {
    #[proto(tag = 1)]
    id: u32,
    #[proto(tag = 2)]
    name: String,
    #[proto(tag = 3)]
    quantity: u32,
}

impl ToProtoType for Item {
    fn proto_type_name() -> &'static str { "Item" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message Item {\n  uint32 id = 1;\n  string name = 2;\n  uint32 quantity = 3;\n}".to_string())
    }
}

// --- Handlers ---

async fn create_item_json(body: Json<CreateItem>) -> Json<Item> {
    Json(Item { id: 1, name: body.0.name, quantity: body.0.quantity })
}

// --- Helpers ---

type ItemAPI = (PostEndpoint<ItemsPath, CreateItem, Item>,);

async fn start_direct_server() -> u16 {
    let direct = into_direct_handler(|req: CreateItem| async move {
        Item { id: 1, name: req.name, quantity: req.quantity }
    });

    let descriptor = <ItemAPI as typeway_grpc::service::ApiToServiceDescriptor>::service_descriptor("ItemService", "test.v1");
    let router = Router::new();
    let mut grpc_router = typeway_server::grpc_dispatch::GrpcRouter::from_router(&router, &descriptor);
    grpc_router.add_direct_handler(
        "/test.v1.ItemService/CreateItem".to_string(),
        direct,
    );

    let multiplexer = typeway_server::grpc_dispatch::GrpcMultiplexer::new(
        RouterService::new(Arc::new(Router::new())),
        Arc::new(grpc_router),
        Arc::new(typeway_grpc::ReflectionService::from_api::<ItemAPI>("ItemService", "test.v1")),
        typeway_grpc::HealthService::new(),
        false,
        None,
        None,
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = multiplexer.clone();
            tokio::spawn(async move {
                let _ = hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
                    .serve_connection(io, hyper_util::service::TowerToHyperService::new(svc))
                    .await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

// --- Tests ---

/// Direct handler produces correct response via binary protobuf.
#[tokio::test]
async fn direct_handler_roundtrip() {
    let port = start_direct_server().await;

    let req = CreateItem { name: "Widget".into(), quantity: 10 };
    let binary = req.encode_to_vec();
    let framed = typeway_grpc::framing::encode_grpc_frame(&binary);

    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .build()
        .unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:{port}/test.v1.ItemService/CreateItem"))
        .header("content-type", "application/grpc+proto")
        .header("te", "trailers")
        .body(framed)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("grpc-status").unwrap(),
        "0"
    );

    let body = resp.bytes().await.unwrap();
    let unframed = typeway_grpc::framing::decode_grpc_frame(&body).unwrap();
    let item = Item::typeway_decode(unframed).unwrap();
    assert_eq!(item.id, 1);
    assert_eq!(item.name, "Widget");
    assert_eq!(item.quantity, 10);
}

/// Direct handler returns error for invalid protobuf.
#[tokio::test]
async fn direct_handler_invalid_input() {
    let port = start_direct_server().await;

    let garbage = typeway_grpc::framing::encode_grpc_frame(&[0xFF, 0xFF, 0xFF]);

    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .build()
        .unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:{port}/test.v1.ItemService/CreateItem"))
        .header("content-type", "application/grpc+proto")
        .header("te", "trailers")
        .body(garbage)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    // Should have non-zero grpc-status (InvalidArgument).
    let status = resp.headers().get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0);
    assert_ne!(status, 0, "expected error status for invalid input");
}

/// Direct handler on unknown method returns UNIMPLEMENTED.
#[tokio::test]
async fn direct_handler_unimplemented() {
    let port = start_direct_server().await;

    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .build()
        .unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:{port}/test.v1.ItemService/DeleteItem"))
        .header("content-type", "application/grpc+proto")
        .header("te", "trailers")
        .body(vec![])
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let status = resp.headers().get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0);
    assert_eq!(status, 12, "expected UNIMPLEMENTED (12)");
}
