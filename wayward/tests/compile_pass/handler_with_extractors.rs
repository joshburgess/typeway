// Handlers with various extractor combinations compile.
use wayward::prelude::*;

wayward_path!(type UsersPath = "users");
wayward_path!(type UserByIdPath = "users" / u32);

#[derive(Clone)]
struct AppState;

#[derive(serde::Serialize)]
struct User {
    id: u32,
}

#[derive(serde::Deserialize)]
struct CreateUser {
    name: String,
}

// No args
async fn list_users() -> Json<Vec<User>> {
    Json(vec![])
}

// Path extractor
async fn get_user(path: Path<UserByIdPath>) -> Json<User> {
    let (id,) = path.0;
    Json(User { id })
}

// State extractor
async fn with_state(state: State<AppState>) -> &'static str {
    let _ = state;
    "ok"
}

// Path + State
async fn path_and_state(_path: Path<UserByIdPath>, _state: State<AppState>) -> &'static str {
    "ok"
}

// Body extractor (last arg)
async fn create_user(body: Json<CreateUser>) -> Json<User> {
    let _ = body;
    Json(User { id: 1 })
}

// State + Body
async fn state_and_body(_state: State<AppState>, body: Json<CreateUser>) -> Json<User> {
    let _ = body;
    Json(User { id: 1 })
}

fn main() {}
