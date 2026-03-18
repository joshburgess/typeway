use std::sync::{Arc, Mutex};

use http::StatusCode;
use serde::{Deserialize, Serialize};

use wayward_client::Client;
use wayward_core::*;
use wayward_macros::*;
use wayward_server::*;

// --- Shared types ---

wayward_path!(type UsersPath = "users");
wayward_path!(type UserByIdPath = "users" / u32);

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

    // Bind to port 0 for a random available port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Spawn the server in the background using the pre-bound listener.
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

    // Give the server a moment to start accepting connections.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    port
}

// --- Tests ---

#[tokio::test]
async fn test_list_users() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let users = client.call::<ListUsersEndpoint>(()).await.unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[1].name, "Bob");
}

#[tokio::test]
async fn test_get_user() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let user = client.call::<GetUserEndpoint>((1u32,)).await.unwrap();
    assert_eq!(
        user,
        User {
            id: 1,
            name: "Alice".into()
        }
    );
}

#[tokio::test]
async fn test_get_user_not_found() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let err = client.call::<GetUserEndpoint>((999u32,)).await.unwrap_err();
    match err {
        wayward_client::ClientError::Status { status, .. } => {
            assert_eq!(status, StatusCode::NOT_FOUND);
        }
        other => panic!("expected Status error, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_create_user() {
    let port = start_server().await;
    let client = Client::new(&format!("http://127.0.0.1:{port}")).unwrap();

    let new_user = client
        .call::<CreateUserEndpoint>((
            (),
            CreateUser {
                name: "Charlie".into(),
            },
        ))
        .await
        .unwrap();
    assert_eq!(new_user.name, "Charlie");
    assert_eq!(new_user.id, 3);

    // Verify it shows up in the list.
    let users = client.call::<ListUsersEndpoint>(()).await.unwrap();
    assert_eq!(users.len(), 3);
}
