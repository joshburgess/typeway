use axum::{
    extract::{Path, State, Json},
    http::StatusCode,
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState {
    db: String,
}

#[derive(Serialize)]
struct User {
    id: u32,
    name: String,
}

#[derive(Deserialize)]
struct CreateUser {
    name: String,
}

async fn list_users(State(state): State<AppState>) -> Json<Vec<User>> {
    let _ = state;
    Json(vec![])
}

async fn get_user(Path(id): Path<u32>, State(state): State<AppState>) -> Json<User> {
    let _ = state;
    Json(User {
        id,
        name: "test".to_string(),
    })
}

async fn create_user(
    State(state): State<AppState>,
    Json(body): Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    let _ = state;
    (
        StatusCode::CREATED,
        Json(User {
            id: 1,
            name: body.name,
        }),
    )
}

async fn delete_user(Path(id): Path<u32>) -> StatusCode {
    let _ = id;
    StatusCode::NO_CONTENT
}

fn app() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", get(get_user).delete(delete_user))
}
