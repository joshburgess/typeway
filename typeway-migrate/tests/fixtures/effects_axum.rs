use axum::{Router, routing::get};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

async fn hello() -> &'static str { "hello" }
async fn health() -> &'static str { "ok" }

fn app() -> Router {
    Router::new()
        .route("/hello", get(hello))
        .route("/health", get(health))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
