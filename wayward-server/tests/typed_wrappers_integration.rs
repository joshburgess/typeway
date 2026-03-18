//! Integration tests for type-level endpoint wrappers.

use std::sync::Arc;
use std::time::Duration;

use wayward_core::*;
use wayward_macros::*;
use wayward_server::typed::*;
use wayward_server::*;

// --- Setup ---

wayward_path!(type UsersPath = "users");
wayward_path!(type TagsPath = "tags");

#[derive(serde::Serialize, serde::Deserialize)]
struct User {
    name: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CreateUser {
    name: String,
}

struct CreateUserValidator;
impl Validate<CreateUser> for CreateUserValidator {
    fn validate(body: &CreateUser) -> Result<(), String> {
        if body.name.is_empty() {
            return Err("name is required".into());
        }
        if body.name.len() < 2 {
            return Err("name must be at least 2 characters".into());
        }
        Ok(())
    }
}

struct V1;
impl ApiVersion for V1 {
    const PREFIX: &'static str = "v1";
}

// --- Handlers ---

async fn get_tags() -> Json<Vec<String>> {
    Json(vec!["rust".into(), "wayward".into()])
}

async fn create_user(body: Json<CreateUser>) -> Json<User> {
    Json(User { name: body.0.name })
}

async fn get_users_v1() -> &'static str {
    "v1 users"
}

async fn create_user_json(body: Json<CreateUser>) -> Json<User> {
    Json(User { name: body.0.name })
}

// --- Test APIs ---

type ValidatedAPI = (
    GetEndpoint<TagsPath, Vec<String>>,
    Validated<CreateUserValidator, PostEndpoint<UsersPath, CreateUser, User>>,
);

type VersionedAPI = (Versioned<V1, GetEndpoint<UsersPath, String>>,);

type ContentTypeAPI = (ContentType<JsonContent, PostEndpoint<UsersPath, CreateUser, User>>,);

// --- Helpers ---

async fn start_server<A, H>(handlers: H) -> u16
where
    A: ApiSpec + Send + 'static,
    H: Serves<A>,
{
    let server = Server::<A>::new(handlers);
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

// ===========================================================================
// Validated tests
// ===========================================================================

#[tokio::test]
async fn validated_passes_valid_body() {
    let port =
        start_server::<ValidatedAPI, _>((bind!(get_tags), bind_validated!(create_user))).await;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/users"))
        .json(&serde_json::json!({"name": "Alice"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["name"], "Alice");
}

#[tokio::test]
async fn validated_rejects_empty_name() {
    let port =
        start_server::<ValidatedAPI, _>((bind!(get_tags), bind_validated!(create_user))).await;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/users"))
        .json(&serde_json::json!({"name": ""}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 422);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("name is required"));
}

#[tokio::test]
async fn validated_rejects_short_name() {
    let port =
        start_server::<ValidatedAPI, _>((bind!(get_tags), bind_validated!(create_user))).await;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/users"))
        .json(&serde_json::json!({"name": "A"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 422);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("at least 2 characters"));
}

#[tokio::test]
async fn validated_rejects_invalid_json() {
    let port =
        start_server::<ValidatedAPI, _>((bind!(get_tags), bind_validated!(create_user))).await;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/users"))
        .header("content-type", "application/json")
        .body("not json")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 422);
}

// ===========================================================================
// Versioned tests
// ===========================================================================

#[tokio::test]
async fn versioned_matches_with_prefix() {
    let port = start_server::<VersionedAPI, _>((bind!(get_users_v1),)).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/v1/users"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "v1 users");
}

#[tokio::test]
async fn versioned_rejects_without_prefix() {
    let port = start_server::<VersionedAPI, _>((bind!(get_users_v1),)).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn versioned_rejects_wrong_version() {
    let port = start_server::<VersionedAPI, _>((bind!(get_users_v1),)).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/v2/users"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

// ===========================================================================
// ContentType tests
// ===========================================================================

#[tokio::test]
async fn content_type_accepts_correct() {
    let port = start_server::<ContentTypeAPI, _>((bind_content_type!(create_user_json),)).await;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/users"))
        .header("content-type", "application/json")
        .json(&serde_json::json!({"name": "Alice"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn content_type_rejects_wrong() {
    let port = start_server::<ContentTypeAPI, _>((bind_content_type!(create_user_json),)).await;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/users"))
        .header("content-type", "text/plain")
        .body(r#"{"name":"Alice"}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 415);
}

#[tokio::test]
async fn content_type_rejects_missing() {
    let port = start_server::<ContentTypeAPI, _>((bind_content_type!(create_user_json),)).await;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/users"))
        .body(r#"{"name":"Alice"}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 415);
}
