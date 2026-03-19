use std::sync::{Arc, Mutex};

use http::StatusCode;
use serde::{Deserialize, Serialize};

use typeway_client::{client_api, Client};
use typeway_core::*;
use typeway_macros::*;
use typeway_server::*;

// --- Shared types ---

typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);

type ListUsersEp = GetEndpoint<UsersPath, Vec<User>>;
type GetUserEp = GetEndpoint<UserByIdPath, User>;
type CreateUserEp = PostEndpoint<UsersPath, CreateUser, User>;
type DeleteUserEp = DeleteEndpoint<UserByIdPath, ()>;

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

// --- Use client_api! to generate a named wrapper ---

client_api! {
    /// A typed client for the Users API.
    pub struct UserClient;

    /// List all users.
    list_users => ListUsersEp;

    /// Get a user by ID.
    get_user => GetUserEp;

    /// Create a new user.
    create_user => CreateUserEp;

    /// Delete a user by ID.
    delete_user => DeleteUserEp;
}

// --- Handlers ---

async fn list_users_handler(state: State<AppState>) -> Json<Vec<User>> {
    let users = state.0.lock().unwrap().clone();
    Json(users)
}

async fn get_user_handler(
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

async fn create_user_handler(
    state: State<AppState>,
    body: Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    let mut users = state.0.lock().unwrap();
    let id = users.len() as u32 + 1;
    let user = User {
        id,
        name: body.0.name,
    };
    users.push(user.clone());
    (StatusCode::CREATED, Json(user))
}

async fn delete_user_handler(
    path: Path<UserByIdPath>,
    state: State<AppState>,
) -> StatusCode {
    let (id,) = path.0;
    let mut users = state.0.lock().unwrap();
    let len_before = users.len();
    users.retain(|u| u.id != id);
    if users.len() < len_before {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

// --- Test server ---

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
        DeleteEndpoint<UserByIdPath, ()>,
    );

    let server = Server::<API>::new((
        bind::<_, _, _>(list_users_handler),
        bind::<_, _, _>(get_user_handler),
        bind::<_, _, _>(create_user_handler),
        bind::<_, _, _>(delete_user_handler),
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

/// Verify the generated struct compiles and delegates correctly.
#[tokio::test]
async fn test_client_api_list_users() {
    let port = start_server().await;
    let client = UserClient::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let users = client.list_users(()).await.unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[1].name, "Bob");
}

#[tokio::test]
async fn test_client_api_get_user() {
    let port = start_server().await;
    let client = UserClient::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let user = client.get_user((1u32,)).await.unwrap();
    assert_eq!(
        user,
        User {
            id: 1,
            name: "Alice".into()
        }
    );
}

#[tokio::test]
async fn test_client_api_create_user() {
    let port = start_server().await;
    let client = UserClient::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let new_user = client
        .create_user((
            (),
            CreateUser {
                name: "Charlie".into(),
            },
        ))
        .await
        .unwrap();
    assert_eq!(new_user.name, "Charlie");
    assert_eq!(new_user.id, 3);
}

#[tokio::test]
async fn test_client_api_get_user_not_found() {
    let port = start_server().await;
    let client = UserClient::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let err = client.get_user((999u32,)).await.unwrap_err();
    match err {
        typeway_client::ClientError::Status { status, .. } => {
            assert_eq!(status, StatusCode::NOT_FOUND);
        }
        other => panic!("expected Status error, got: {other:?}"),
    }
}

/// Verify `from_client` constructor works.
#[tokio::test]
async fn test_client_api_from_client() {
    let port = start_server().await;
    let inner = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();
    let client = UserClient::from_client(inner);

    let users = client.list_users(()).await.unwrap();
    assert_eq!(users.len(), 2);
}

/// Verify streaming returns a raw response.
#[tokio::test]
async fn test_call_streaming() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let resp = client.call_streaming::<ListUsersEp>(()).await.unwrap();
    assert!(resp.status().is_success());

    let body = resp.text().await.unwrap();
    let users: Vec<User> = serde_json::from_str(&body).unwrap();
    assert_eq!(users.len(), 2);
}

/// Verify streaming returns an error on non-2xx.
#[tokio::test]
async fn test_call_streaming_not_found() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let err = client
        .call_streaming::<GetUserEp>((999u32,))
        .await
        .unwrap_err();
    match err {
        typeway_client::ClientError::Status { status, .. } => {
            assert_eq!(status, StatusCode::NOT_FOUND);
        }
        other => panic!("expected Status error, got: {other:?}"),
    }
}
