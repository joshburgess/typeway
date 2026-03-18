//! # Wayward RealWorld Example App
//!
//! A full implementation of the [RealWorld](https://github.com/gothinkster/realworld)
//! ("Conduit") API spec using wayward.
//!
//! ## Running
//!
//! 1. Start PostgreSQL and create the database:
//!    ```sh
//!    createdb realworld
//!    ```
//!
//! 2. Set environment variables (or use defaults):
//!    ```sh
//!    export DATABASE_HOST=localhost
//!    export DATABASE_PORT=5432
//!    export DATABASE_NAME=realworld
//!    export DATABASE_USER=postgres
//!    export DATABASE_PASSWORD=postgres
//!    ```
//!
//! 3. Run the server:
//!    ```sh
//!    cargo run -p wayward-realworld
//!    ```
//!
//! The server runs on `http://localhost:3000` with OpenAPI docs at `/docs`.
//!
//! ## Frontend
//!
//! Any [RealWorld frontend](https://codebase.show/projects/realworld) works
//! with this backend. Point it at `http://localhost:3000/api`.

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
    .layer(CorsLayer::permissive())
    .layer(RequestIdLayer::new());

    println!("Wayward RealWorld API running on http://localhost:3000");
    println!("  API:     http://localhost:3000/api");
    println!("  OpenAPI: (enable with .with_openapi())");
    println!();
    println!("19 endpoints covering users, profiles, articles, comments, tags");

    server.serve("0.0.0.0:3000".parse().unwrap()).await.unwrap();
}
