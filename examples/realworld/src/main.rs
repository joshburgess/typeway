//! # Typeway RealWorld Example App
//!
//! A full implementation of the [RealWorld](https://github.com/gothinkster/realworld)
//! ("Conduit") API spec using typeway, with an Elm + Tailwind frontend.
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
//!    cargo run -p typeway-realworld
//!    ```
//!
//! Open `http://localhost:3000` for the full app.

mod api;
mod auth;
mod db;
mod handlers;
mod models;

use typeway_server::request_id::RequestIdLayer;
use typeway_server::tower_http::cors::CorsLayer;
use typeway_server::{bind, bind_auth, Server};

use api::RealWorldAPI;

#[tokio::main]
async fn main() {
    let pool = db::create_pool().await;
    db::run_migrations(&pool).await;
    db::seed_data(&pool).await;

    // Frontend static files directory.
    let frontend_dir = std::env::var("FRONTEND_DIR")
        .unwrap_or_else(|_| "examples/realworld/frontend/public".to_string());

    // Build the typeway API server with all 19 endpoints.
    // Static file serving and SPA fallback are built into typeway — no Axum needed.
    let server = Server::<RealWorldAPI>::new((
        // Auth (public)
        bind!(handlers::register),
        bind!(handlers::login),
        // Auth (protected — compiler enforces AuthUser as first arg)
        bind_auth!(handlers::get_current_user),
        bind_auth!(handlers::update_user),
        // Profiles
        bind!(handlers::get_profile),
        bind_auth!(handlers::follow_profile),
        bind_auth!(handlers::unfollow_profile),
        // Articles
        bind!(handlers::list_articles),
        bind_auth!(handlers::get_feed),
        bind!(handlers::get_article),
        bind_auth!(handlers::create_article),
        bind_auth!(handlers::update_article),
        bind_auth!(handlers::delete_article),
        // Favorites
        bind_auth!(handlers::favorite_article),
        bind_auth!(handlers::unfavorite_article),
        // Comments
        bind!(handlers::get_comments),
        bind_auth!(handlers::add_comment),
        bind_auth!(handlers::delete_comment),
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

    println!("Typeway Word running on http://localhost:{port}");
    println!("  Frontend: http://localhost:{port}/");
    println!("  API:      http://localhost:{port}/api/");
    println!("  Static:   {frontend_dir}");
    println!();
    println!("19 API endpoints + Elm frontend — pure typeway, no Axum");

    server.serve(addr.parse().unwrap()).await.unwrap();
}
