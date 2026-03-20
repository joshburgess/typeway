//! Integration tests for unified REST+gRPC serving (Phase C).
//!
//! These tests require the `grpc` feature flag.

#![cfg(feature = "grpc")]

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use typeway_core::endpoint::{DeleteEndpoint, GetEndpoint, PostEndpoint};
use typeway_core::path::{Capture, HCons, HNil, Lit, LitSegment};
use typeway_grpc::mapping::ToProtoType;
use typeway_server::*;

// --- Path types ---

#[allow(non_camel_case_types)]
struct users;
impl LitSegment for users {
    const VALUE: &'static str = "users";
}

type UsersPath = HCons<Lit<users>, HNil>;
type UserByIdPath = HCons<Lit<users>, HCons<Capture<u32>, HNil>>;

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

async fn delete_user(path: Path<UserByIdPath>, state: State<AppState>) -> http::StatusCode {
    let (id,) = path.0;
    let mut all = state.0.lock().unwrap();
    if let Some(pos) = all.iter().position(|u| u.id == id) {
        all.remove(pos);
        http::StatusCode::NO_CONTENT
    } else {
        http::StatusCode::NOT_FOUND
    }
}

// --- API type ---

type TestAPI = (
    GetEndpoint<UsersPath, Vec<User>>,
    GetEndpoint<UserByIdPath, User>,
    PostEndpoint<UsersPath, CreateUser, User>,
    DeleteEndpoint<UserByIdPath, ()>,
);

// --- Helper to start a GrpcServer ---

async fn start_grpc_server() -> u16 {
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

    let grpc_server = Server::<TestAPI>::new((
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
        bind::<_, _, _>(delete_user),
    ))
    .with_state(state)
    .with_grpc("UserService", "users.v1");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        grpc_server
            .serve_with_shutdown(listener, std::future::pending())
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

// --- Tests ---

/// Verify that `Server::with_grpc` compiles and produces a GrpcServer.
#[test]
fn server_with_grpc_compiles() {
    let state: AppState = Arc::new(std::sync::Mutex::new(vec![]));

    // This just needs to compile — no need to serve.
    let _grpc_server = Server::<TestAPI>::new((
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
        bind::<_, _, _>(delete_user),
    ))
    .with_state(state)
    .with_grpc("UserService", "users.v1");
}

/// GrpcServer should serve normal REST requests (non-gRPC).
#[tokio::test]
async fn grpc_server_serves_rest() {
    let port = start_grpc_server().await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let user_list: Vec<User> = resp.json().await.unwrap();
    assert_eq!(user_list.len(), 2);
    assert_eq!(user_list[0].name, "Alice");
    assert_eq!(user_list[1].name, "Bob");
}

/// GrpcServer should serve REST GET with path captures.
#[tokio::test]
async fn grpc_server_serves_rest_with_captures() {
    let port = start_grpc_server().await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users/1"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let user: User = resp.json().await.unwrap();
    assert_eq!(user.id, 1);
    assert_eq!(user.name, "Alice");
}

/// Sending a gRPC request (content-type: application/grpc+json) to a known
/// method should return grpc-status: 0 (OK).
#[tokio::test]
async fn grpc_server_serves_grpc() {
    let port = start_grpc_server().await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{port}/users.v1.UserService/ListUser"
        ))
        .header("content-type", "application/grpc+json")
        .body("")
        .send()
        .await
        .unwrap();

    // gRPC always returns HTTP 200.
    assert_eq!(resp.status(), 200);

    // grpc-status: 0 means OK.
    let grpc_status = resp
        .headers()
        .get("grpc-status")
        .expect("missing grpc-status header")
        .to_str()
        .unwrap();
    assert_eq!(grpc_status, "0");

    // content-type should be application/grpc+json.
    let ct = resp
        .headers()
        .get("content-type")
        .expect("missing content-type header")
        .to_str()
        .unwrap();
    assert!(ct.starts_with("application/grpc"));
}

/// Sending a gRPC request to an unknown method should return
/// grpc-status: 12 (UNIMPLEMENTED).
#[tokio::test]
async fn grpc_unknown_method_returns_unimplemented() {
    let port = start_grpc_server().await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{port}/users.v1.UserService/UpdateUser"
        ))
        .header("content-type", "application/grpc+json")
        .body("")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let grpc_status = resp
        .headers()
        .get("grpc-status")
        .expect("missing grpc-status header")
        .to_str()
        .unwrap();
    assert_eq!(grpc_status, "12"); // UNIMPLEMENTED
}

/// A REST 404 (user not found) should be translated to grpc-status: 5 (NOT_FOUND)
/// when accessed via the gRPC bridge.
#[tokio::test]
async fn grpc_404_maps_to_not_found() {
    let port = start_grpc_server().await;

    let client = reqwest::Client::new();
    // GetUser maps to GET /users/{}, but since gRPC doesn't pass path params
    // in the same way, the bridge uses the rest_path which has a placeholder.
    // The descriptor's rest_path is "/users/{}" — so this will hit the REST
    // router at /users/{} which is a literal path and won't match any user.
    //
    // For the current simplified bridge, the request is routed to the rest_path
    // as-is. With rest_path="/users/{}" the router won't match a real user,
    // producing a 404 → grpc-status 5.
    let resp = client
        .post(format!(
            "http://127.0.0.1:{port}/users.v1.UserService/GetUser"
        ))
        .header("content-type", "application/grpc+json")
        .body("")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let grpc_status = resp
        .headers()
        .get("grpc-status")
        .expect("missing grpc-status header")
        .to_str()
        .unwrap();
    // The rest_path "/users/{}" won't match any u32 capture, so the router
    // returns 404, which maps to grpc-status 5 (NOT_FOUND).
    assert_eq!(grpc_status, "5");
}

/// Verify that `.with_state()` works on GrpcServer.
#[tokio::test]
async fn grpc_server_with_state() {
    let state: AppState = Arc::new(std::sync::Mutex::new(vec![User {
        id: 42,
        name: "StatefulUser".into(),
    }]));

    let grpc_server = Server::<TestAPI>::new((
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
        bind::<_, _, _>(delete_user),
    ))
    .with_state(state)
    .with_grpc("UserService", "users.v1");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        grpc_server
            .serve_with_shutdown(listener, std::future::pending())
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify state is accessible via REST.
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let user_list: Vec<User> = resp.json().await.unwrap();
    assert_eq!(user_list.len(), 1);
    assert_eq!(user_list[0].id, 42);
    assert_eq!(user_list[0].name, "StatefulUser");
}

/// Verify the service descriptor is accessible from GrpcServer.
#[test]
fn grpc_server_exposes_service_descriptor() {
    let state: AppState = Arc::new(std::sync::Mutex::new(vec![]));

    let grpc_server = Server::<TestAPI>::new((
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
        bind::<_, _, _>(delete_user),
    ))
    .with_state(state)
    .with_grpc("UserService", "users.v1");

    let desc = grpc_server.service_descriptor();
    assert_eq!(desc.name, "UserService");
    assert_eq!(desc.package, "users.v1");
    assert_eq!(desc.methods.len(), 4);

    // Verify method names.
    let names: Vec<&str> = desc.methods.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"ListUser"));
    assert!(names.contains(&"GetUser"));
    assert!(names.contains(&"CreateUser"));
    assert!(names.contains(&"DeleteUser"));
}

/// Verify that `.with_grpc()` can be chained after `.with_state()` on Server.
#[test]
fn with_state_then_with_grpc() {
    let state: AppState = Arc::new(std::sync::Mutex::new(vec![]));

    let _grpc_server = Server::<TestAPI>::new((
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
        bind::<_, _, _>(delete_user),
    ))
    .with_state(state)
    .with_grpc("Svc", "pkg.v1");
}
