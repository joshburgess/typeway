use axum::{
    extract::{State, Json},
    http::StatusCode,
    routing::post,
    Router,
};
use serde::Deserialize;

#[derive(Clone)]
struct AppState { db: String }

#[derive(Deserialize)]
struct CreateUser { username: String, email: String, password: String }

#[derive(serde::Serialize)]
struct UserResponse { id: u32, username: String }

async fn register(
    State(state): State<AppState>,
    Json(body): Json<CreateUser>,
) -> Result<Json<UserResponse>, (StatusCode, String)> {
    if body.username.is_empty() {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, "username required".into()));
    }
    if body.email.is_empty() {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, "email required".into()));
    }
    if body.password.len() < 6 {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, "password too short".into()));
    }
    let _ = state;
    Ok(Json(UserResponse { id: 1, username: body.username }))
}

fn app() -> Router<AppState> {
    Router::new()
        .route("/users", post(register))
}
