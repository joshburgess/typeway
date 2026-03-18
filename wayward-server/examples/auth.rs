//! Demonstrates authentication middleware using a custom extractor.
//!
//! Run: cargo run -p wayward-server --example auth
//! Test:
//!   curl http://127.0.0.1:3000/public              # 200 — no auth needed
//!   curl http://127.0.0.1:3000/protected            # 401 — missing token
//!   curl -H "Authorization: Bearer admin-token" \
//!        http://127.0.0.1:3000/protected            # 200 — authenticated

use wayward_core::*;
use wayward_macros::*;
use wayward_server::*;

// ---------------------------------------------------------------------------
// Auth extractor — validates Bearer tokens
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AuthUser {
    username: String,
    role: String,
}

impl FromRequestParts for AuthUser {
    type Error = JsonError;

    fn from_request_parts(parts: &http::request::Parts) -> Result<Self, Self::Error> {
        let token = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| JsonError::unauthorized("missing or invalid Authorization header"))?;

        // In production, verify against a database/JWT/etc.
        match token {
            "admin-token" => Ok(AuthUser {
                username: "admin".into(),
                role: "admin".into(),
            }),
            "user-token" => Ok(AuthUser {
                username: "user".into(),
                role: "user".into(),
            }),
            _ => Err(JsonError::unauthorized("invalid token")),
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

wayward_path!(type PublicPath = "public");
wayward_path!(type ProtectedPath = "protected");
wayward_path!(type AdminPath = "admin");

type API = (
    GetEndpoint<PublicPath, String>,
    GetEndpoint<ProtectedPath, String>,
    GetEndpoint<AdminPath, String>,
);

async fn public_handler() -> &'static str {
    "This is public — no auth needed"
}

async fn protected_handler(user: AuthUser) -> String {
    format!("Hello, {}! You have role: {}", user.username, user.role)
}

async fn admin_handler(user: AuthUser) -> Result<String, JsonError> {
    if user.role != "admin" {
        return Err(JsonError::forbidden("admin access required"));
    }
    Ok(format!("Welcome to the admin panel, {}!", user.username))
}

#[tokio::main]
async fn main() {
    let server = Server::<API>::new((
        bind!(public_handler),
        bind!(protected_handler),
        bind!(admin_handler),
    ));

    println!("Auth example on http://127.0.0.1:3000");
    println!("  GET /public    — no auth");
    println!("  GET /protected — requires Bearer token");
    println!("  GET /admin     — requires admin token");

    server
        .serve("127.0.0.1:3000".parse().unwrap())
        .await
        .unwrap();
}
