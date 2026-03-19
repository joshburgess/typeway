//! Full CRUD API example using the wayward facade crate.
//!
//! Run: cargo run -p typeway --features full --example users_api
//! Test: curl http://localhost:3000/users

use std::sync::{Arc, Mutex};

use typeway::http::StatusCode;
use typeway::prelude::*;

// ---- Path types ----

typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);

// ---- API type ----

type UsersAPI = (
    GetEndpoint<UsersPath, Json<Vec<User>>>,
    GetEndpoint<UserByIdPath, Json<User>>,
    PostEndpoint<UsersPath, Json<CreateUser>, Json<User>>,
    DeleteEndpoint<UserByIdPath, StatusCode>,
);

// ---- Domain types ----

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct CreateUser {
    name: String,
    email: String,
}

type AppState = Arc<Mutex<Vec<User>>>;

// ---- Handlers ----

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
        email: body.0.email,
    };
    users.push(user.clone());
    (StatusCode::CREATED, Json(user))
}

async fn delete_user(path: Path<UserByIdPath>, state: State<AppState>) -> StatusCode {
    let (id,) = path.0;
    let mut users = state.0.lock().unwrap();
    if let Some(pos) = users.iter().position(|u| u.id == id) {
        users.remove(pos);
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

// ---- Main ----

#[tokio::main]
async fn main() {
    let state: AppState = Arc::new(Mutex::new(vec![
        User {
            id: 1,
            name: "Alice".into(),
            email: "alice@example.com".into(),
        },
        User {
            id: 2,
            name: "Bob".into(),
            email: "bob@example.com".into(),
        },
    ]));

    // The compiler ensures every endpoint in UsersAPI has a handler.
    // Remove any handler → compile error.
    let server = Server::<UsersAPI>::new((
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
        bind::<_, _, _>(delete_user),
    ))
    .with_state(state);

    println!("Wayward Users API running on http://0.0.0.0:3000");
    println!("  GET    /users      - list all users");
    println!("  GET    /users/:id  - get user by id");
    println!("  POST   /users      - create user");
    println!("  DELETE /users/:id  - delete user");

    server.serve("0.0.0.0:3000".parse().unwrap()).await.unwrap();
}
