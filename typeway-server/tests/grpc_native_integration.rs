//! Integration tests for native gRPC dispatch (Phase 1).
//!
//! These tests start a real server with `.with_native()` and verify
//! that gRPC requests are dispatched directly to handlers with real
//! HTTP/2 trailers.

#![cfg(feature = "grpc-native")]

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use typeway_core::endpoint::{GetEndpoint, PostEndpoint};
use typeway_core::path::{Capture, HCons, HNil, Lit, LitSegment};
use typeway_grpc::mapping::ToProtoType;
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
    .with_native();

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

/// Verify that `.with_native()` compiles.
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
    .with_native();
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
