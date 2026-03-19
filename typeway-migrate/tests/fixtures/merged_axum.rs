use axum::{
    extract::{Path, State, Json},
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState { db: String }

#[derive(Serialize)]
struct User { id: u32, name: String }

#[derive(Serialize)]
struct Article { id: u32, title: String }

#[derive(Deserialize)]
struct CreateUser { name: String }

async fn list_users(State(state): State<AppState>) -> Json<Vec<User>> {
    let _ = state;
    Json(vec![])
}

async fn get_user(Path(id): Path<u32>, State(state): State<AppState>) -> Json<User> {
    let _ = state;
    Json(User { id, name: "test".to_string() })
}

async fn create_user(State(state): State<AppState>, Json(body): Json<CreateUser>) -> Json<User> {
    let _ = state;
    Json(User { id: 1, name: body.name })
}

async fn list_articles(State(state): State<AppState>) -> Json<Vec<Article>> {
    let _ = state;
    Json(vec![])
}

async fn delete_article(Path(id): Path<u32>) -> axum::http::StatusCode {
    let _ = id;
    axum::http::StatusCode::NO_CONTENT
}

fn user_routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", get(get_user))
}

fn article_routes() -> Router<AppState> {
    Router::new()
        .route("/articles", get(list_articles))
        .route("/articles/{id}", delete(delete_article))
}

fn app() -> Router<AppState> {
    Router::new()
        .merge(user_routes())
        .merge(article_routes())
}
