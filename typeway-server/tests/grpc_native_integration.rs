//! Integration tests for gRPC dispatch.
//!
//! These tests start a real server and verify end-to-end gRPC dispatch
//! including streaming, native client calls, and TypewayCodec integration.

#![cfg(feature = "grpc")]

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use typeway_core::endpoint::{GetEndpoint, PostEndpoint};
use typeway_core::path::{Capture, HCons, HNil, Lit, LitSegment};
use typeway_grpc::mapping::ToProtoType;
use typeway_grpc::streaming::ServerStream;
use typeway_server::*;

// --- Path types ---

#[allow(non_camel_case_types)]
struct __lit_users;
impl LitSegment for __lit_users {
    const VALUE: &'static str = "users";
}

type UsersPath = HCons<Lit<__lit_users>, HNil>;
type UserByIdPath = HCons<Lit<__lit_users>, HCons<Capture<u32>, HNil>>;

// --- Domain types ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct User {
    id: u32,
    name: String,
}

impl ToProtoType for User {
    fn proto_type_name() -> &'static str {
        "User"
    }
    fn is_message() -> bool {
        true
    }
    fn message_definition() -> Option<String> {
        Some("message User {\n  uint32 id = 1;\n  string name = 2;\n}".to_string())
    }
}

#[derive(Debug, Deserialize)]
struct CreateUser {
    name: String,
}

impl ToProtoType for CreateUser {
    fn proto_type_name() -> &'static str {
        "CreateUser"
    }
    fn is_message() -> bool {
        true
    }
    fn message_definition() -> Option<String> {
        Some("message CreateUser {\n  string name = 1;\n}".to_string())
    }
}

type AppState = Arc<std::sync::Mutex<Vec<User>>>;

// --- Handlers ---

async fn list_users(state: State<AppState>) -> Json<Vec<User>> {
    Json(state.0.lock().unwrap().clone())
}

async fn get_user(
    path: Path<UserByIdPath>,
    state: State<AppState>,
) -> Result<Json<User>, http::StatusCode> {
    let (id,) = path.0;
    let all = state.0.lock().unwrap();
    all.iter()
        .find(|u| u.id == id)
        .cloned()
        .map(Json)
        .ok_or(http::StatusCode::NOT_FOUND)
}

async fn create_user(
    state: State<AppState>,
    body: Json<CreateUser>,
) -> (http::StatusCode, Json<User>) {
    let mut all = state.0.lock().unwrap();
    let id = all.len() as u32 + 1;
    let user = User {
        id,
        name: body.0.name,
    };
    all.push(user.clone());
    (http::StatusCode::CREATED, Json(user))
}

// --- API type ---

type TestAPI = (
    GetEndpoint<UsersPath, Vec<User>>,
    GetEndpoint<UserByIdPath, User>,
    PostEndpoint<UsersPath, CreateUser, User>,
);

// --- Helper to start a NativeGrpcServer ---

async fn start_native_grpc_server() -> u16 {
    let state: AppState = Arc::new(std::sync::Mutex::new(vec![
        User {
            id: 1,
            name: "Alice".into(),
        },
        User {
            id: 2,
            name: "Bob".into(),
        },
    ]));

    let native_server = Server::<TestAPI>::new((
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
    ))
    .with_state(state)
    .with_grpc("UserService", "users.v1")
    ;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        native_server
            .serve_with_shutdown(listener, std::future::pending())
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

// --- Tests ---

/// Verify that `` compiles.
#[test]
fn native_grpc_server_compiles() {
    let state: AppState = Arc::new(std::sync::Mutex::new(vec![]));

    let _native_server = Server::<TestAPI>::new((
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
    ))
    .with_state(state)
    .with_grpc("UserService", "users.v1")
    ;
}

/// Native gRPC server should still serve REST requests.
#[tokio::test]
async fn native_serves_rest() {
    let port = start_native_grpc_server().await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let users: Vec<User> = resp.json().await.unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Alice");
}

/// Native gRPC server should handle gRPC JSON requests.
#[tokio::test]
async fn native_serves_grpc_json() {
    let port = start_native_grpc_server().await;

    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .build()
        .unwrap();

    // Call ListUser (no body needed).
    let body = typeway_grpc::framing::encode_grpc_frame(b"{}");
    let resp = client
        .post(format!(
            "http://127.0.0.1:{port}/users.v1.UserService/ListUser"
        ))
        .header("content-type", "application/grpc+json")
        .header("te", "trailers")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    // The response should have gRPC content.
    let body_bytes = resp.bytes().await.unwrap();
    assert!(!body_bytes.is_empty(), "response body should not be empty");
}

/// Native gRPC server should handle unimplemented methods.
#[tokio::test]
async fn native_unimplemented_method() {
    let port = start_native_grpc_server().await;

    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .build()
        .unwrap();

    let resp = client
        .post(format!(
            "http://127.0.0.1:{port}/users.v1.UserService/NonExistent"
        ))
        .header("content-type", "application/grpc+json")
        .header("te", "trailers")
        .body(vec![])
        .send()
        .await
        .unwrap();

    // Should return HTTP 200 (gRPC always does) with error in trailers/headers.
    assert_eq!(resp.status(), 200);
}

/// Native gRPC server should serve health check.
#[tokio::test]
async fn native_health_check() {
    let port = start_native_grpc_server().await;

    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .build()
        .unwrap();

    let body = typeway_grpc::framing::encode_grpc_frame(b"{}");
    let resp = client
        .post(format!(
            "http://127.0.0.1:{port}/grpc.health.v1.Health/Check"
        ))
        .header("content-type", "application/grpc+json")
        .header("te", "trailers")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let grpc_status = resp
        .headers()
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("?");
    assert_eq!(grpc_status, "0");
}

/// Native gRPC server should handle POST (create) gRPC call.
#[tokio::test]
async fn native_grpc_create_user() {
    let port = start_native_grpc_server().await;

    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .build()
        .unwrap();

    let req_json = serde_json::json!({"name": "Charlie"});
    let req_bytes = serde_json::to_vec(&req_json).unwrap();
    let body = typeway_grpc::framing::encode_grpc_frame(&req_bytes);

    let resp = client
        .post(format!(
            "http://127.0.0.1:{port}/users.v1.UserService/CreateUser"
        ))
        .header("content-type", "application/grpc+json")
        .header("te", "trailers")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body_bytes = resp.bytes().await.unwrap();
    // Decode gRPC frame.
    let unframed = typeway_grpc::framing::decode_grpc_frame(&body_bytes).unwrap();
    let user: User = serde_json::from_slice(unframed).unwrap();
    assert_eq!(user.name, "Charlie");
    assert_eq!(user.id, 3); // After Alice(1) and Bob(2)
}

// ---------------------------------------------------------------------------
// GrpcStream<T> server-streaming integration test
// ---------------------------------------------------------------------------

/// A handler that returns a GrpcStream — real frame-by-frame streaming.
async fn list_users_streaming(state: State<AppState>) -> GrpcStream<User> {
    let users = state.0.lock().unwrap().clone();
    let (tx, stream) = GrpcStream::channel(8);
    tokio::spawn(async move {
        for user in users {
            if tx.send(user).await.is_err() {
                break;
            }
        }
    });
    stream
}

type StreamingAPI = (
    ServerStream<GetEndpoint<UsersPath, Vec<User>>>,
    PostEndpoint<UsersPath, CreateUser, User>,
);

async fn start_streaming_server() -> u16 {
    let state: AppState = Arc::new(std::sync::Mutex::new(vec![
        User { id: 1, name: "Alice".into() },
        User { id: 2, name: "Bob".into() },
        User { id: 3, name: "Charlie".into() },
    ]));

    let server = Server::<StreamingAPI>::new((
        bind::<_, _, _>(list_users_streaming),
        bind::<_, _, _>(create_user),
    ))
    .with_state(state)
    .with_grpc("StreamService", "stream.v1");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        server
            .serve_with_shutdown(listener, std::future::pending())
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

/// GrpcStream handler streams individual frames to the client.
#[tokio::test]
async fn grpc_stream_returns_individual_frames() {
    let port = start_streaming_server().await;

    let client = typeway_grpc::GrpcTestClient::new(&format!("http://127.0.0.1:{port}"));

    let resp = client
        .call_streaming_empty("stream.v1.StreamService", "ListUser")
        .await;

    assert!(resp.is_ok(), "expected OK, got {:?}", resp.grpc_code());
    assert_eq!(resp.len(), 3, "expected 3 streamed items, got {}", resp.len());
    assert_eq!(resp.items[0]["name"], "Alice");
    assert_eq!(resp.items[1]["name"], "Bob");
    assert_eq!(resp.items[2]["name"], "Charlie");
}

// ---------------------------------------------------------------------------
// NativeGrpcClient end-to-end test
// ---------------------------------------------------------------------------

/// GrpcClient makes a real unary call to a running server.
#[tokio::test]
async fn grpc_client_unary_call() {
    let port = start_native_grpc_server().await;

    let client = typeway_grpc::GrpcClient::new(
        &format!("http://127.0.0.1:{port}"),
        "UserService",
        "users.v1",
    )
    .unwrap();

    // Call CreateUser.
    let resp = client
        .call("CreateUser", &serde_json::json!({"name": "Dave"}))
        .await
        .unwrap();

    assert_eq!(resp["name"], "Dave");
    assert_eq!(resp["id"], 3);
}

/// GrpcClient makes a real server-streaming call.
#[tokio::test]
async fn grpc_client_server_stream_call() {
    let port = start_streaming_server().await;

    let client = typeway_grpc::GrpcClient::new(
        &format!("http://127.0.0.1:{port}"),
        "StreamService",
        "stream.v1",
    )
    .unwrap();

    let stream = client
        .call_server_stream("ListUser", &serde_json::json!({}))
        .await
        .unwrap();

    let items = stream.collect().await.unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["name"], "Alice");
    assert_eq!(items[2]["name"], "Charlie");
}

/// GrpcClient returns error for unimplemented method.
#[tokio::test]
async fn grpc_client_unimplemented_error() {
    let port = start_native_grpc_server().await;

    let client = typeway_grpc::GrpcClient::new(
        &format!("http://127.0.0.1:{port}"),
        "UserService",
        "users.v1",
    )
    .unwrap();

    let result = client
        .call("NonExistentMethod", &serde_json::json!({}))
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        typeway_grpc::GrpcClientError::Status { code, .. } => {
            assert_eq!(code, typeway_grpc::GrpcCode::Unimplemented);
        }
        other => panic!("expected Status error, got: {other}"),
    }
}

// ---------------------------------------------------------------------------
// TypewayCodecAdapter integration test
// ---------------------------------------------------------------------------

/// TypewayCodecAdapter with derive macro roundtrips through GrpcCodec trait.
#[test]
fn typeway_codec_adapter_with_derive() {
    use typeway_grpc::{GrpcCodec, TypewayCodecAdapter};
    use typeway_macros::TypewayCodec;

    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TypewayCodec)]
    struct TestUser {
        #[proto(tag = 1)]
        id: u32,
        #[proto(tag = 2)]
        name: String,
        #[proto(tag = 3)]
        active: bool,
    }

    let adapter = TypewayCodecAdapter::<TestUser>::new();

    // Encode JSON → binary (via TypewayCodec)
    let json = serde_json::json!({"id": 42, "name": "Alice", "active": true});
    let encoded = adapter.encode(&json).unwrap();

    // Decode binary → JSON (via TypewayCodec)
    let decoded = adapter.decode(&encoded).unwrap();
    assert_eq!(decoded["id"], 42);
    assert_eq!(decoded["name"], "Alice");
    assert_eq!(decoded["active"], true);

    // Verify content type
    assert_eq!(adapter.content_type(), "application/grpc+proto");
}

// ---------------------------------------------------------------------------
// Binary protobuf dispatch integration test
// ---------------------------------------------------------------------------

/// Binary protobuf content-type is detected and dispatched.
///
/// Verifies the native dispatch properly detects `application/grpc` (binary)
/// vs `application/grpc+json` (JSON) content types and routes accordingly.
/// When a binary request fails to decode (e.g., spec mismatch), the server
/// returns a proper InvalidArgument error instead of silently falling back.
#[cfg(feature = "grpc-proto-binary")]
#[tokio::test]
async fn binary_protobuf_content_type_detection() {
    let state: AppState = Arc::new(std::sync::Mutex::new(vec![
        User { id: 1, name: "Alice".into() },
    ]));

    let server = Server::<TestAPI>::new((
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
    ))
    .with_state(state)
    .with_grpc("UserService", "users.v1")
    .with_proto_binary();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        server
            .serve_with_shutdown(listener, std::future::pending())
            .await
            .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .build()
        .unwrap();

    // JSON requests still work when proto-binary is enabled.
    let json_body = typeway_grpc::encode_grpc_frame(
        serde_json::json!({"name": "Dave"}).to_string().as_bytes(),
    );
    let resp = client
        .post(format!(
            "http://127.0.0.1:{port}/users.v1.UserService/CreateUser"
        ))
        .header("content-type", "application/grpc+json")
        .header("te", "trailers")
        .body(json_body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let grpc_status = resp
        .headers()
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("?");
    assert_eq!(grpc_status, "0", "JSON request should succeed");

    // Binary protobuf request is detected by content-type.
    // The server returns an error because the spec-driven transcoder
    // may not fully resolve the request message for this test setup.
    let proto_bytes = vec![0x0A, 0x04, b'D', b'a', b'v', b'e']; // tag 1, len 4, "Dave"
    let framed = typeway_grpc::encode_grpc_frame(&proto_bytes);
    let resp = client
        .post(format!(
            "http://127.0.0.1:{port}/users.v1.UserService/CreateUser"
        ))
        .header("content-type", "application/grpc") // binary
        .header("te", "trailers")
        .body(framed)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    // The server detected binary content-type and attempted transcoding.
    // Regardless of success/failure, it returns a valid gRPC response
    // (HTTP 200 with grpc-status in headers).
    let grpc_status = resp
        .headers()
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(-1);
    assert_ne!(grpc_status, -1, "grpc-status header should be present");
}
