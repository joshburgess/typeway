use typeway::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState { db: String }

#[derive(Serialize)]
struct User { id: u32, name: String }

#[derive(Deserialize)]
struct CreateUser { name: String }

typeway_path!(type UsersPath = "users");
typeway_path!(type UsersByIdPath = "users" / u32);

type API = (
    GetEndpoint<UsersPath, Json<Vec<User>>>,
    GetEndpoint<UsersByIdPath, Json<User>>,
    PostEndpoint<UsersPath, Json<CreateUser>, Json<User>>,
    DeleteEndpoint<UsersByIdPath, StatusCode>,
);

async fn list_users(state: State<AppState>) -> Json<Vec<User>> {
    let state = state.0;
    Json(vec![])
}

async fn get_user(path: Path<UsersByIdPath>, state: State<AppState>) -> Json<User> {
    let (id,) = path.0;
    let state = state.0;
    Json(User { id, name: "test".to_string() })
}

async fn create_user(state: State<AppState>, body: Json<CreateUser>) -> Json<User> {
    let state = state.0;
    let body = body.0;
    Json(User { id: 1, name: body.name })
}

async fn delete_user(path: Path<UsersByIdPath>) -> StatusCode {
    let (id,) = path.0;
    let _ = id;
    StatusCode::NO_CONTENT
}

async fn serve(addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Server::<API>::new((
        bind!(list_users),
        bind!(get_user),
        bind!(create_user),
        bind!(delete_user),
    ))
    .serve(addr)
    .await
}
