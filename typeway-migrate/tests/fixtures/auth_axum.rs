use axum::{
    extract::{Path, State, Json, Query},
    http::StatusCode,
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState { db: String }

#[derive(Serialize)]
struct User { id: u32, name: String }

#[derive(Deserialize)]
struct CreateUser { name: String }

#[derive(Deserialize)]
struct Pagination { page: u32, limit: u32 }

// This is a custom auth extractor — the migration tool should detect it
struct AuthUser(u32);

// Public endpoint — no auth
async fn list_users(
    Query(pagination): Query<Pagination>,
    State(state): State<AppState>,
) -> Json<Vec<User>> {
    let _ = (pagination, state);
    Json(vec![])
}

// Protected endpoint — AuthUser is first arg
async fn get_user(
    auth: AuthUser,
    Path(id): Path<u32>,
    State(state): State<AppState>,
) -> Json<User> {
    let _ = (auth, state);
    Json(User { id, name: "test".to_string() })
}

// Protected endpoint with body
async fn create_user(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    let _ = (auth, state);
    (StatusCode::CREATED, Json(User { id: 1, name: body.name }))
}

// Protected delete
async fn delete_user(auth: AuthUser, Path(id): Path<u32>) -> StatusCode {
    let _ = (auth, id);
    StatusCode::NO_CONTENT
}

fn app() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", get(get_user).delete(delete_user))
}
