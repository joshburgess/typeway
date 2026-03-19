use std::sync::{Arc, Mutex};

use http::StatusCode;
use serde::{Deserialize, Serialize};

use typeway_client::{Client, TypedResponse};
use typeway_core::*;
use typeway_macros::*;
use typeway_server::*;

// --- Shared types ---

typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);

type ListUsersEndpoint = GetEndpoint<UsersPath, Vec<User>>;
type GetUserEndpoint = GetEndpoint<UserByIdPath, User>;
type CreateUserEndpoint = PostEndpoint<UsersPath, CreateUser, User>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct User {
    id: u32,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateUser {
    name: String,
}

type AppState = Arc<Mutex<Vec<User>>>;

// --- Handlers ---

async fn list_users(state: State<AppState>) -> Json<Vec<User>> {
    let users = state.0.lock().unwrap().clone();
    Json(users)
}

async fn get_user(
    path: Path<UserByIdPath>,
    state: State<AppState>,
) -> Result<Json<User>, StatusCode> {
    let (id,) = path.0;
    let users = state.0.lock().unwrap();
    users
        .iter()
        .find(|u| u.id == id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_user(state: State<AppState>, body: Json<CreateUser>) -> (StatusCode, Json<User>) {
    let mut users = state.0.lock().unwrap();
    let id = users.len() as u32 + 1;
    let user = User {
        id,
        name: body.0.name,
    };
    users.push(user.clone());
    (StatusCode::CREATED, Json(user))
}

// --- Start test server on a random port ---

async fn start_server() -> u16 {
    let state: AppState = Arc::new(Mutex::new(vec![
        User {
            id: 1,
            name: "Alice".into(),
        },
        User {
            id: 2,
            name: "Bob".into(),
        },
    ]));

    type API = (
        GetEndpoint<UsersPath, Vec<User>>,
        GetEndpoint<UserByIdPath, User>,
        PostEndpoint<UsersPath, CreateUser, User>,
    );

    let server = Server::<API>::new((
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
    ))
    .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let router = Arc::new(server.into_router());
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
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    port
}

// --- Tests ---

#[tokio::test]
async fn test_request_builder_with_custom_header() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    // The custom header doesn't affect the response, but verifies the builder
    // sends the request successfully with the extra header applied.
    let user = client
        .request::<GetUserEndpoint>((1u32,))
        .header(
            http::header::HeaderName::from_static("x-custom"),
            http::header::HeaderValue::from_static("test-value"),
        )
        .send()
        .await
        .unwrap();

    assert_eq!(
        user,
        User {
            id: 1,
            name: "Alice".into()
        }
    );
}

#[tokio::test]
async fn test_request_builder_send_full_returns_metadata() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let resp: TypedResponse<User> = client
        .request::<GetUserEndpoint>((1u32,))
        .send_full()
        .await
        .unwrap();

    assert_eq!(resp.status, StatusCode::OK);
    assert_eq!(
        resp.body,
        User {
            id: 1,
            name: "Alice".into()
        }
    );
    // The server sets content-type for JSON responses.
    let ct = resp.header("content-type");
    assert!(ct.is_some(), "expected content-type header in response");
}

#[tokio::test]
async fn test_request_builder_with_query_params() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    // Query params are appended to the URL. The server ignores them for this
    // endpoint, but we verify the request still succeeds (no URL corruption).
    let users = client
        .request::<ListUsersEndpoint>(())
        .query("page", "2")
        .query("limit", "10")
        .send()
        .await
        .unwrap();

    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_request_builder_timeout_compiles_and_sends() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    // Verify that setting a timeout compiles and the request succeeds
    // (the server responds quickly so no timeout occurs).
    let user = client
        .request::<GetUserEndpoint>((1u32,))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .unwrap();

    assert_eq!(user.name, "Alice");
}

#[tokio::test]
async fn test_call_full_returns_metadata() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let resp = client
        .call_full::<ListUsersEndpoint>(())
        .await
        .unwrap();

    assert_eq!(resp.status, StatusCode::OK);
    assert_eq!(resp.body.len(), 2);
    assert!(resp.header("content-type").is_some());
}

#[tokio::test]
async fn test_request_builder_post_with_body_and_header() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let resp = client
        .request::<CreateUserEndpoint>((
            (),
            CreateUser {
                name: "Charlie".into(),
            },
        ))
        .header(
            http::header::HeaderName::from_static("x-trace-id"),
            http::header::HeaderValue::from_static("trace-123"),
        )
        .send_full()
        .await
        .unwrap();

    // The server returns 201 Created for new users.
    assert_eq!(resp.status, StatusCode::CREATED);
    assert_eq!(resp.body.name, "Charlie");
    assert_eq!(resp.body.id, 3);
}
