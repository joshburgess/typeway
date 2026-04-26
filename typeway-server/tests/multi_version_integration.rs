//! Integration tests for multiple API versions served simultaneously.

use std::sync::Arc;
use std::time::Duration;

use typeway_core::*;
use typeway_macros::*;
use typeway_server::typed::*;
use typeway_server::*;

// --- Setup ---

typeway_path!(type UsersPath = "users");

#[derive(serde::Serialize)]
struct UserV1 {
    name: String,
}
#[derive(serde::Serialize)]
struct UserV2 {
    name: String,
    email: String,
}

struct V1;
impl ApiVersion for V1 {
    const PREFIX: &'static str = "v1";
}

struct V2;
impl ApiVersion for V2 {
    const PREFIX: &'static str = "v2";
}

// Two versioned endpoints in the same API.
type API = (
    Versioned<V1, GetEndpoint<UsersPath, Vec<UserV1>>>,
    Versioned<V2, GetEndpoint<UsersPath, Vec<UserV2>>>,
);

async fn get_users_v1() -> Json<Vec<UserV1>> {
    Json(vec![UserV1 {
        name: "Alice".into(),
    }])
}

async fn get_users_v2() -> Json<Vec<UserV2>> {
    Json(vec![UserV2 {
        name: "Alice".into(),
        email: "alice@example.com".into(),
    }])
}

async fn start_server() -> u16 {
    let server = Server::<API>::new((bind!(get_users_v1), bind!(get_users_v2)));
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
async fn v1_returns_v1_response() {
    let port = start_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/v1/users"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let users = body.as_array().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["name"], "Alice");
    assert!(users[0].get("email").is_none());
}

#[tokio::test]
async fn v2_returns_v2_response() {
    let port = start_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/v2/users"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let users = body.as_array().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["name"], "Alice");
    assert_eq!(users[0]["email"], "alice@example.com");
}

#[tokio::test]
async fn unversioned_path_returns_404() {
    let port = start_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn wrong_version_returns_404() {
    let port = start_server().await;
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/v3/users"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
