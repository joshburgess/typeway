//! # Versioned API with Compile-Time Effect Enforcement
//!
//! Demonstrates two typeway features working together:
//!
//! 1. **Type-level API versioning** — V2 is defined as a set of typed changes
//!    applied to V1. The changelog is a type, not documentation.
//!
//! 2. **Compile-time middleware effects** — V2 endpoints require rate limiting.
//!    If you forget to apply the rate limiter, the code won't compile.
//!
//! ## Run
//!
//! ```bash
//! cargo run -p typeway-versioned-api
//! ```
//!
//! ## Test
//!
//! ```bash
//! # V1 endpoints (no rate limiting required):
//! curl http://localhost:3000/users
//! curl http://localhost:3000/users/1
//!
//! # V2 endpoints (rate limiting enforced at compile time):
//! curl http://localhost:3000/users/1/profile
//! curl http://localhost:3000/health
//! ```

use std::time::Duration;

use serde::Serialize;

use typeway_core::effects::*;
use typeway_core::endpoint::*;
use typeway_core::path::{Capture, HCons, HNil, Lit, LitSegment};
use typeway_core::versioning::*;
use typeway_server::*;

// =========================================================================
// Path types
// =========================================================================

#[allow(non_camel_case_types)]
struct __lit_users;
impl LitSegment for __lit_users {
    const VALUE: &'static str = "users";
}

#[allow(non_camel_case_types)]
struct __lit_profile;
impl LitSegment for __lit_profile {
    const VALUE: &'static str = "profile";
}

#[allow(non_camel_case_types)]
struct __lit_health;
impl LitSegment for __lit_health {
    const VALUE: &'static str = "health";
}

type UsersPath = HCons<Lit<__lit_users>, HNil>;
type UserByIdPath = HCons<Lit<__lit_users>, HCons<Capture<u32>, HNil>>;
type UserProfilePath = HCons<Lit<__lit_users>, HCons<Capture<u32>, HCons<Lit<__lit_profile>, HNil>>>;
type HealthPath = HCons<Lit<__lit_health>, HNil>;

// =========================================================================
// Domain types
// =========================================================================

/// V1 user — basic fields.
#[derive(Debug, Clone, Serialize)]
struct UserV1 {
    id: u32,
    name: String,
}

/// V2 user — adds email (non-breaking: new field with default).
#[derive(Debug, Clone, Serialize)]
struct UserV2 {
    id: u32,
    name: String,
    email: String,
}

/// User profile — new in V2.
#[derive(Debug, Clone, Serialize)]
struct UserProfile {
    id: u32,
    name: String,
    email: String,
    bio: String,
    joined: String,
}

/// Health check response — new in V2.
#[derive(Debug, Clone, Serialize)]
struct HealthCheck {
    status: String,
    version: String,
    uptime_seconds: u64,
}

// =========================================================================
// V1 API — the original
// =========================================================================

/// V1: two endpoints, no middleware requirements.
type V1 = (
    GetEndpoint<UsersPath, Vec<UserV1>>,
    GetEndpoint<UserByIdPath, UserV1>,
);

// =========================================================================
// V2 Changes — typed delta from V1
// =========================================================================

/// The changelog is a TYPE. Not a markdown file. Not a comment. A type
/// that the compiler can inspect, count, and enforce.
type V2Changes = (
    // UserV1 → UserV2 (adds email field)
    Replaced<GetEndpoint<UserByIdPath, UserV1>, GetEndpoint<UserByIdPath, UserV2>>,
    // New endpoint: user profiles
    Added<GetEndpoint<UserProfilePath, UserProfile>>,
    // New endpoint: health check (requires rate limiting)
    Added<Requires<RateLimitRequired, GetEndpoint<HealthPath, HealthCheck>>>,
    // V1 list endpoint deprecated (still works, but flagged)
    Deprecated<GetEndpoint<UsersPath, Vec<UserV1>>>,
);

/// The resolved V2 API after applying changes.
type V2Resolved = (
    GetEndpoint<UsersPath, Vec<UserV1>>,          // deprecated but still present
    GetEndpoint<UserByIdPath, UserV2>,             // replaced: V1 → V2
    GetEndpoint<UserProfilePath, UserProfile>,     // added
    Requires<RateLimitRequired, GetEndpoint<HealthPath, HealthCheck>>,  // added with effect
);

/// V2 = V1 + changes, resolving to V2Resolved.
type V2 = VersionedApi<V1, V2Changes, V2Resolved>;

// =========================================================================
// Handlers
// =========================================================================

async fn list_users() -> Json<Vec<UserV1>> {
    Json(vec![
        UserV1 { id: 1, name: "Alice".into() },
        UserV1 { id: 2, name: "Bob".into() },
        UserV1 { id: 3, name: "Charlie".into() },
    ])
}

async fn get_user(path: Path<UserByIdPath>) -> Result<Json<UserV2>, http::StatusCode> {
    let (id,) = path.0;
    match id {
        1 => Ok(Json(UserV2 { id: 1, name: "Alice".into(), email: "alice@example.com".into() })),
        2 => Ok(Json(UserV2 { id: 2, name: "Bob".into(), email: "bob@example.com".into() })),
        3 => Ok(Json(UserV2 { id: 3, name: "Charlie".into(), email: "charlie@example.com".into() })),
        _ => Err(http::StatusCode::NOT_FOUND),
    }
}

async fn get_profile(path: Path<UserProfilePath>) -> Result<Json<UserProfile>, http::StatusCode> {
    let (id,) = path.0;
    match id {
        1 => Ok(Json(UserProfile {
            id: 1,
            name: "Alice".into(),
            email: "alice@example.com".into(),
            bio: "Rust enthusiast and type theory nerd.".into(),
            joined: "2024-01-15".into(),
        })),
        _ => Err(http::StatusCode::NOT_FOUND),
    }
}

async fn health_check() -> Json<HealthCheck> {
    Json(HealthCheck {
        status: "ok".into(),
        version: "2.0.0".into(),
        uptime_seconds: 42,
    })
}

// =========================================================================
// Main — compile-time enforcement in action
// =========================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    // Print the changelog at startup — it's derived from the type.
    tracing::info!("API Changelog V1 → V2:");
    tracing::info!("  Added:      {}", V2Changes::ADDED);
    tracing::info!("  Removed:    {}", V2Changes::REMOVED);
    tracing::info!("  Replaced:   {}", V2Changes::REPLACED);
    tracing::info!("  Deprecated: {}", V2Changes::DEPRECATED);

    // Build the V2 server with effect enforcement.
    //
    // The health check endpoint requires RateLimitRequired.
    // If we forget .provide::<RateLimitRequired>(), this won't compile:
    //
    //   EffectfulServer::<V2>::new(handlers)
    //       .ready()  // ← ERROR: RateLimitRequired not provided
    //
    // Uncomment the .provide() and .layer() lines below to see the
    // compile error when the rate limiter is missing.

    tracing::info!("Starting V2 server on http://localhost:3000");
    tracing::info!("  GET /users          — list users (deprecated in V2)");
    tracing::info!("  GET /users/:id      — get user (V2: includes email)");
    tracing::info!("  GET /users/:id/profile — user profile (new in V2)");
    tracing::info!("  GET /health         — health check (requires rate limiter)");

    EffectfulServer::<V2>::new((
        bind::<_, _, _>(list_users),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(get_profile),
        bind::<_, _, _>(health_check),
    ))
    .provide::<RateLimitRequired>()  // discharge the effect
    .layer(tower_http::timeout::TimeoutLayer::with_status_code(
        http::StatusCode::REQUEST_TIMEOUT,
        Duration::from_secs(30),
    ))
    .ready()  // compiles only because RateLimitRequired is provided
    .serve("0.0.0.0:3000".parse()?)
    .await
}
