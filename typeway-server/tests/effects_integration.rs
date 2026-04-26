//! Integration tests for the type-level middleware effects system.
//!
//! Verifies that `EffectfulServer` correctly tracks provided effects and
//! that `serve()`/`ready()` only compile when all requirements are met.

use std::sync::Arc;
use std::time::Duration;

use typeway_core::effects::*;
use typeway_core::*;
use typeway_macros::*;
use typeway_server::*;

// --- Path types ---

typeway_path!(type UsersPath = "users");
typeway_path!(type HealthPath = "health");

// --- Domain types ---

#[derive(serde::Serialize, serde::Deserialize)]
struct User {
    name: String,
}

// --- Handlers ---

async fn get_users() -> Json<Vec<User>> {
    Json(vec![User {
        name: "Alice".into(),
    }])
}

async fn health() -> &'static str {
    "ok"
}

// --- API types ---

/// An API where some endpoints require effects.
type EffectfulAPI = (
    Requires<AuthRequired, GetEndpoint<UsersPath, Vec<User>>>,
    GetEndpoint<HealthPath, String>,
);

/// An API with multiple effects on different endpoints.
type MultiEffectAPI = (
    Requires<AuthRequired, GetEndpoint<UsersPath, Vec<User>>>,
    Requires<CorsRequired, GetEndpoint<HealthPath, String>>,
);

/// An API with nested effects on a single endpoint.
type NestedEffectAPI =
    (Requires<CorsRequired, Requires<AuthRequired, GetEndpoint<UsersPath, Vec<User>>>>,);

/// An API with no effects (plain endpoints).
type NoEffectAPI = (
    GetEndpoint<UsersPath, Vec<User>>,
    GetEndpoint<HealthPath, String>,
);

// --- Helpers ---

#[allow(clippy::type_complexity)]
async fn start_effectful_server(
    handlers: (
        BoundHandler<Requires<AuthRequired, GetEndpoint<UsersPath, Vec<User>>>>,
        BoundHandler<GetEndpoint<HealthPath, String>>,
    ),
) -> u16 {
    let server = EffectfulServer::<EffectfulAPI>::new(handlers).provide::<AuthRequired>();

    let inner_server = server.ready();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let router = Arc::new(inner_server.into_router());
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = RouterService::new(router.clone());
            let hyper_svc = hyper_util::service::TowerToHyperService::new(svc);
            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, hyper_svc)
                    .await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

// --- Compile-time tests ---

/// Verify that `EffectfulServer` with a single-effect API compiles
/// when the effect is provided.
#[tokio::test]
async fn effectful_server_compiles_with_effect_provided() {
    let port = start_effectful_server((bind!(get_users), bind!(health))).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/health"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
}

/// Verify the effectful server correctly routes requests.
#[tokio::test]
async fn effectful_server_routes_requests() {
    let port = start_effectful_server((bind!(get_users), bind!(health))).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let users: Vec<User> = resp.json().await.unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "Alice");
}

/// Verify that `ready()` works for multi-effect APIs when all effects are provided.
#[test]
fn multi_effect_ready_compiles() {
    fn assert_compiles() {
        // This function body is never called — it only needs to compile.
        let _server = EffectfulServer::<MultiEffectAPI>::new((bind!(get_users), bind!(health)))
            .provide::<AuthRequired>()
            .provide::<CorsRequired>()
            .ready();
    }
    // The test passes if it compiles.
    let _ = assert_compiles;
}

/// Verify that nested effects (Requires<A, Requires<B, E>>) work.
#[test]
fn nested_effects_ready_compiles() {
    fn assert_compiles() {
        let _server = EffectfulServer::<NestedEffectAPI>::new((bind!(get_users),))
            .provide::<AuthRequired>()
            .provide::<CorsRequired>()
            .ready();
    }
    let _ = assert_compiles;
}

/// Verify that no-effect APIs work with EffectfulServer (ready() with ENil).
#[test]
fn no_effect_api_ready_compiles_immediately() {
    fn assert_compiles() {
        let _server =
            EffectfulServer::<NoEffectAPI>::new((bind!(get_users), bind!(health))).ready();
    }
    let _ = assert_compiles;
}

/// Verify that `.provide()` order does not matter.
#[test]
fn provide_order_does_not_matter() {
    fn cors_first() {
        let _server = EffectfulServer::<MultiEffectAPI>::new((bind!(get_users), bind!(health)))
            .provide::<CorsRequired>()
            .provide::<AuthRequired>()
            .ready();
    }

    fn auth_first() {
        let _server = EffectfulServer::<MultiEffectAPI>::new((bind!(get_users), bind!(health)))
            .provide::<AuthRequired>()
            .provide::<CorsRequired>()
            .ready();
    }

    let _ = cors_first;
    let _ = auth_first;
}

/// Verify that extra effects can be provided without harm.
#[test]
fn extra_effects_are_harmless() {
    fn assert_compiles() {
        let _server = EffectfulServer::<EffectfulAPI>::new((bind!(get_users), bind!(health)))
            .provide::<AuthRequired>()
            .provide::<CorsRequired>() // not required but harmless
            .provide::<TracingRequired>() // not required but harmless
            .ready();
    }
    let _ = assert_compiles;
}

/// Verify that EffectfulServer works with `.layer()`.
#[test]
fn effectful_server_with_layer_compiles() {
    fn assert_compiles() {
        let _server = EffectfulServer::<EffectfulAPI>::new((bind!(get_users), bind!(health)))
            .provide::<AuthRequired>()
            .layer(tower_http::cors::CorsLayer::permissive())
            .ready();
    }
    let _ = assert_compiles;
}

/// Verify that layered effectful server supports chained provide + layer.
#[test]
fn layered_effectful_server_provide_compiles() {
    fn assert_compiles() {
        let _server = EffectfulServer::<MultiEffectAPI>::new((bind!(get_users), bind!(health)))
            .provide::<AuthRequired>()
            .layer(tower_http::cors::CorsLayer::permissive())
            .provide::<CorsRequired>()
            .ready();
    }
    let _ = assert_compiles;
}
