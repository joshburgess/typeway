//! # Typeway RealWorld Example App
//!
//! A full implementation of the [RealWorld](https://github.com/gothinkster/realworld)
//! ("Conduit") API spec using typeway, with an Elm + Tailwind frontend.
//!
//! ## Advanced features demonstrated
//!
//! 1. **Effects system** (`EffectfulServer` + `Requires<CorsRequired, _>`):
//!    Public endpoints declare a CORS middleware requirement. Comment out
//!    `.provide::<CorsRequired>()` below and it fails to compile.
//!
//! 2. **Content negotiation** (`NegotiatedResponse`):
//!    `GET /api/tags` and `GET /api/articles/:slug` return JSON, plain text,
//!    or XML based on the `Accept` header.
//!    Try: `curl -H "Accept: application/xml" http://localhost:4000/api/tags`
//!
//! 3. **API versioning** (`VersionedApi` + `assert_api_compatible!`):
//!    V1 → V2 → V3 evolution with typed deltas. V2 adds search + health,
//!    replaces tags, deprecates login. V3 adds stats, upgrades user response,
//!    removes deprecated login. See `api.rs`.
//!
//! 4. **Session-typed WebSocket** (`TypedWebSocket<FeedProtocol>`):
//!    Protocol for live article updates is encoded as a session type.
//!    See `handlers::ws_feed`.
//!
//! 5. **Validation** (`Validated<V, E>` + `bind_validated!`):
//!    Registration and article creation validated before the handler runs.
//!
//! 6. **gRPC** (`.with_grpc()` + `.with_grpc_docs()` + `#[derive(ToProtoType)]`):
//!    All 22 REST endpoints are also available as gRPC methods on the same port.
//!    Set `GENERATE_PROTO=1` to emit `realworld.proto` and `realworld-grpc-spec.json`.
//!    Visit `/grpc-docs` for HTML documentation, `/grpc-spec` for the JSON spec.
//!
//! ## Running
//!
//! ```sh
//! createdb realworld
//! cd examples/realworld/frontend && elm make src/Main.elm --output=public/elm.js && cd ../../..
//! cargo run -p typeway-realworld
//! ```

mod api;
mod auth;
mod db;
mod handlers;
mod models;

use typeway_grpc::spec::ApiToGrpcSpec;
use typeway_grpc::ApiToProto;
use typeway_server::bind_validated;
use typeway_server::tower_http::cors::CorsLayer;
use typeway_server::{bind, bind_auth, EffectfulServer};

use api::RealWorldAPI;

#[tokio::main]
async fn main() {
    let pool = db::create_pool().await;
    db::run_migrations(&pool).await;
    db::seed_data(&pool).await;

    let frontend_dir = std::env::var("FRONTEND_DIR")
        .unwrap_or_else(|_| "examples/realworld/frontend/public".to_string());

    // ---------------------------------------------------------------------------
    // EffectfulServer: the API type contains `Requires<CorsRequired, _>` on
    // several endpoints. The server won't compile until all effects are provided.
    //
    // TRY: Comment out `.provide::<CorsRequired>()` and run `cargo check`.
    // ---------------------------------------------------------------------------
    let server = EffectfulServer::<RealWorldAPI>::new((
        // Registration: Validated<NewUserValidator, _> runs validation before handler.
        // bind_validated! creates a BoundHandler that enforces this at runtime.
        bind_validated!(handlers::register),
        // (V3: login endpoint removed — no handler needed)
        // Protected endpoints: compiler enforces AuthUser as first handler arg.
        // V3: get_current_user now returns UserResponseV3 with created_at + articles_count.
        bind_auth!(handlers::get_current_user_v3),
        bind_auth!(handlers::update_user),
        // Public read endpoints wrapped in Requires<CorsRequired, _>.
        bind!(handlers::get_profile),
        bind_auth!(handlers::follow_profile),
        bind_auth!(handlers::unfollow_profile),
        bind!(handlers::list_articles),
        bind_auth!(handlers::get_feed),
        bind!(handlers::get_article),
        // Article creation: Protected + Validated (auth + body validation).
        bind_auth!(handlers::create_article),
        bind_auth!(handlers::update_article),
        bind_auth!(handlers::delete_article),
        bind_auth!(handlers::favorite_article),
        bind_auth!(handlers::unfavorite_article),
        bind!(handlers::get_comments),
        bind_auth!(handlers::add_comment),
        bind_auth!(handlers::delete_comment),
        // Tags: V2 handler with counts + content negotiation (JSON, text, XML).
        bind!(handlers::get_tags_v2),
        // V2 additions: health check and article search.
        bind!(handlers::health),
        bind!(handlers::search_articles),
        // V3 additions: site statistics and account deletion.
        bind!(handlers::get_stats),
        bind_auth!(handlers::delete_account),
    ))
    // Provide the CorsRequired effect. The type says "I need CORS", and
    // .provide() satisfies the compile-time check. The actual CorsLayer is
    // applied below via .layer() on the GrpcServer, wrapping the multiplexer.
    //
    // TRY: Comment out `.provide::<CorsRequired>()` and run `cargo check`.
    .provide::<typeway_core::effects::CorsRequired>()
    // .ready() converts to a regular Server. Only compiles if all effects
    // in the API type have been provided.
    .ready()
    .with_state(pool)
    .with_static_files("/static", &frontend_dir)
    .with_spa_fallback(format!("{frontend_dir}/index.html"));

    // Generate .proto file and gRPC spec on request (for demonstration).
    if std::env::var("GENERATE_PROTO").is_ok() {
        let proto = RealWorldAPI::to_proto("RealWorldService", "realworld.v1");
        std::fs::write("realworld.proto", &proto).unwrap();
        println!("Generated realworld.proto");

        let spec = RealWorldAPI::grpc_spec("RealWorldService", "realworld.v1");
        let spec_json = serde_json::to_string_pretty(&spec).unwrap();
        std::fs::write("realworld-grpc-spec.json", &spec_json).unwrap();
        println!("Generated realworld-grpc-spec.json");
    }

    let port = std::env::var("PORT").unwrap_or_else(|_| "4000".to_string());
    let addr = format!("0.0.0.0:{port}");

    println!("Typeway Word running on http://localhost:{port}");
    println!("  Frontend:   http://localhost:{port}/");
    println!("  API:        http://localhost:{port}/api/");
    println!("  Health:     http://localhost:{port}/api/health");
    println!("  Search:     http://localhost:{port}/api/articles/search?q=type");
    println!("  Stats:      http://localhost:{port}/api/stats");
    println!("  gRPC:       grpc+json on the same port");
    println!("  gRPC docs:  http://localhost:{port}/grpc-docs");
    println!("  gRPC spec:  http://localhost:{port}/grpc-spec");
    println!("  Static:     {frontend_dir}");
    println!();
    println!("22 endpoints (V3) + Elm frontend — 7 advanced features:");
    println!("  1. Effects:     CorsRequired enforced at compile time");
    println!("  2. Negotiation: curl -H 'Accept: application/xml' localhost:{port}/api/tags");
    println!("  3. Versioning:  V1->V2->V3 with assert_api_compatible! (api.rs)");
    println!("  4. WebSocket:   Session-typed protocol (handlers.rs::ws_feed)");
    println!("  5. Validation:  Registration + article creation (api.rs)");
    println!("  6. XML support: Tags + articles support JSON/text/XML negotiation");
    println!("  7. gRPC:        Same handlers serve REST and gRPC (grpc+json bridge)");

    server
        .with_grpc("RealWorldService", "realworld.v1")
        .with_grpc_docs()
        .layer(CorsLayer::permissive())
        .serve(addr.parse().unwrap())
        .await
        .unwrap();
}
