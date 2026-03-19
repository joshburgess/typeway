//! Integration tests for route matching, request extraction, and response encoding.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use typeway_core::*;
use typeway_macros::*;
use typeway_server::*;

// --- Types ---

typeway_path!(type HelloPath = "hello");
typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);
typeway_path!(type UserPostsPath = "users" / u32 / "posts" / u32);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct User {
    id: u32,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CreateUser {
    name: String,
}

type AppState = Arc<std::sync::Mutex<Vec<User>>>;

// --- Handlers ---

async fn hello() -> &'static str {
    "hello"
}

async fn get_user(
    path: Path<UserByIdPath>,
    state: State<AppState>,
) -> Result<Json<User>, http::StatusCode> {
    let (id,) = path.0;
    let users = state.0.lock().unwrap();
    users
        .iter()
        .find(|u| u.id == id)
        .cloned()
        .map(Json)
        .ok_or(http::StatusCode::NOT_FOUND)
}

async fn list_users(state: State<AppState>) -> Json<Vec<User>> {
    Json(state.0.lock().unwrap().clone())
}

async fn create_user(
    state: State<AppState>,
    body: Json<CreateUser>,
) -> (http::StatusCode, Json<User>) {
    let mut users = state.0.lock().unwrap();
    let id = users.len() as u32 + 1;
    let user = User {
        id,
        name: body.0.name,
    };
    users.push(user.clone());
    (http::StatusCode::CREATED, Json(user))
}

async fn delete_user(path: Path<UserByIdPath>, state: State<AppState>) -> http::StatusCode {
    let (id,) = path.0;
    let mut users = state.0.lock().unwrap();
    if let Some(pos) = users.iter().position(|u| u.id == id) {
        users.remove(pos);
        http::StatusCode::NO_CONTENT
    } else {
        http::StatusCode::NOT_FOUND
    }
}

async fn get_user_post(path: Path<UserPostsPath>) -> String {
    let (user_id, post_id) = path.0;
    format!("user={user_id} post={post_id}")
}

// --- Test server ---

type TestAPI = (
    GetEndpoint<HelloPath, String>,
    GetEndpoint<UsersPath, Vec<User>>,
    GetEndpoint<UserByIdPath, User>,
    PostEndpoint<UsersPath, CreateUser, User>,
    DeleteEndpoint<UserByIdPath, ()>,
    GetEndpoint<UserPostsPath, String>,
);

async fn start_test_server() -> u16 {
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

    let server = Server::<TestAPI>::new((
        bind::<_, _, _>(hello),
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
        bind::<_, _, _>(delete_user),
        bind::<_, _, _>(get_user_post),
    ))
    .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let router = Arc::new(server.into_router());
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = RouterService::new(router.clone());
            let hyper_svc = hyper_util::service::TowerToHyperService::new(svc);
            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, hyper_svc)
                    .await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

// --- Route matching tests ---

#[tokio::test]
async fn exact_path_match() {
    let port = start_test_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "hello");
}

#[tokio::test]
async fn path_with_single_capture() {
    let port = start_test_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users/1"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let user: User = resp.json().await.unwrap();
    assert_eq!(
        user,
        User {
            id: 1,
            name: "Alice".into()
        }
    );
}

#[tokio::test]
async fn path_with_multiple_captures() {
    let port = start_test_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users/42/posts/7"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "user=42 post=7");
}

#[tokio::test]
async fn method_mismatch_returns_405() {
    let port = start_test_server().await;
    let resp = reqwest::Client::new()
        .delete(format!("http://127.0.0.1:{port}/hello"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 405);
}

#[tokio::test]
async fn unknown_path_returns_404() {
    let port = start_test_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/nonexistent"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn capture_type_parse_failure_returns_404() {
    let port = start_test_server().await;
    // u32 capture can't parse "abc"
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users/abc"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// --- Request extraction tests ---

#[tokio::test]
async fn json_body_deserialization() {
    let port = start_test_server().await;
    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/users"))
        .json(&serde_json::json!({"name": "Charlie"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let user: User = resp.json().await.unwrap();
    assert_eq!(user.name, "Charlie");
    assert_eq!(user.id, 3);
}

#[tokio::test]
async fn malformed_json_returns_400() {
    let port = start_test_server().await;
    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/users"))
        .header("content-type", "application/json")
        .body("not json")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// --- Response encoding tests ---

#[tokio::test]
async fn json_response_has_correct_content_type() {
    let port = start_test_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
}

#[tokio::test]
async fn string_response_has_text_content_type() {
    let port = start_test_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/hello"))
        .await
        .unwrap();
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/plain; charset=utf-8"
    );
}

#[tokio::test]
async fn status_only_response_has_no_body() {
    let port = start_test_server().await;
    let resp = reqwest::Client::new()
        .delete(format!("http://127.0.0.1:{port}/users/1"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
}

#[tokio::test]
async fn not_found_on_missing_resource() {
    let port = start_test_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users/999"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// --- Body size limit tests ---

async fn start_limited_server() -> u16 {
    typeway_path!(type BodyPath = "body");
    type BodyAPI = (PostEndpoint<BodyPath, String, String>,);

    async fn echo_body(body: String) -> String {
        body
    }

    let server = Server::<BodyAPI>::new((bind::<_, _, _>(echo_body),)).max_body_size(64);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let router = Arc::new(server.into_router());
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = RouterService::new(router.clone());
            let hyper_svc = hyper_util::service::TowerToHyperService::new(svc);
            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, hyper_svc)
                    .await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

#[tokio::test]
async fn body_within_limit_succeeds() {
    let port = start_limited_server().await;
    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/body"))
        .body("small")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "small");
}

#[tokio::test]
async fn body_exceeding_limit_returns_413() {
    let port = start_limited_server().await;
    let big_body = "x".repeat(128); // 128 bytes > 64 byte limit
    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/body"))
        .body(big_body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 413);
}
