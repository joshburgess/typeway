//! # Wayward RealWorld Example App
//!
//! A full implementation of the [RealWorld](https://github.com/gothinkster/realworld)
//! ("Conduit") API spec using wayward, with an Elm + Tailwind frontend.
//!
//! ## Running
//!
//! 1. Start PostgreSQL and create the database:
//!    ```sh
//!    createdb realworld
//!    ```
//!
//! 2. Build the Elm frontend (requires `elm` installed):
//!    ```sh
//!    cd examples/realworld/frontend
//!    elm make src/Main.elm --output=public/elm.js
//!    ```
//!
//! 3. Run the server (from the workspace root):
//!    ```sh
//!    cargo run -p wayward-realworld
//!    ```
//!
//! Open `http://localhost:3000` for the full app.

mod api;
mod auth;
mod db;
mod handlers;
mod models;

use wayward_server::request_id::RequestIdLayer;
use wayward_server::tower_http::cors::CorsLayer;
use wayward_server::{bind, Server};

use api::RealWorldAPI;

#[tokio::main]
async fn main() {
    let pool = db::create_pool().await;
    db::run_migrations(&pool).await;
    db::seed_data(&pool).await;

    // Frontend static files directory.
    let frontend_dir = std::env::var("FRONTEND_DIR")
        .unwrap_or_else(|_| "examples/realworld/frontend/public".to_string());

    // Build the wayward API server with all 19 endpoints.
    // Static file serving and SPA fallback are built into wayward — no Axum needed.
    let server = Server::<RealWorldAPI>::new((
        // Auth
        bind!(handlers::register),
        bind!(handlers::login),
        bind!(handlers::get_current_user),
        bind!(handlers::update_user),
        // Profiles
        bind!(handlers::get_profile),
        bind!(handlers::follow_profile),
        bind!(handlers::unfollow_profile),
        // Articles
        bind!(handlers::list_articles),
        bind!(handlers::get_feed),
        bind!(handlers::get_article),
        bind!(handlers::create_article),
        bind!(handlers::update_article),
        bind!(handlers::delete_article),
        // Favorites
        bind!(handlers::favorite_article),
        bind!(handlers::unfavorite_article),
        // Comments
        bind!(handlers::get_comments),
        bind!(handlers::add_comment),
        bind!(handlers::delete_comment),
        // Tags
        bind!(handlers::get_tags),
    ))
    .with_state(pool)
    .with_static_files("/static", &frontend_dir)
    .with_spa_fallback(format!("{frontend_dir}/index.html"))
    .layer(CorsLayer::permissive())
    .layer(RequestIdLayer::new());

    let port = std::env::var("PORT").unwrap_or_else(|_| "4000".to_string());
    let addr = format!("0.0.0.0:{port}");

    println!("Wayward Word running on http://localhost:{port}");
    println!("  Frontend: http://localhost:{port}/");
    println!("  API:      http://localhost:{port}/api/");
    println!("  Static:   {frontend_dir}");
    println!();
    println!("19 API endpoints + Elm frontend — pure wayward, no Axum");

    server.serve(addr.parse().unwrap()).await.unwrap();
}
