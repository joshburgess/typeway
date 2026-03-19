use axum::{
    extract::{Path, State, Json},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState {
    db: String,
}

#[derive(Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
}

#[derive(Deserialize)]
struct CreateUser {
    name: String,
}

struct CustomAuth(String);

#[axum::async_trait]
impl<S> axum::extract::FromRequestParts<S> for CustomAuth
where
    S: Send + Sync,
{
    type Rejection = StatusCode;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(CustomAuth("user".to_string()))
    }
}

async fn list_users(State(state): State<AppState>) -> Json<Vec<User>> {
    let _ = state;
    Json(vec![])
}

async fn get_user(Path(id): Path<u32>) -> impl IntoResponse {
    Json(User {
        id,
        name: "test".to_string(),
    })
}

async fn create_user(
    auth: CustomAuth,
    Json(body): Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    let _ = auth;
    (
        StatusCode::CREATED,
        Json(User {
            id: 1,
            name: body.name,
        }),
    )
}

async fn auth_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> impl IntoResponse {
    next.run(req).await
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", get(get_user))
}

fn app() -> Router<AppState> {
    Router::new()
        .nest("/api/v1", api_routes())
        .layer(middleware::from_fn(auth_middleware))
}
