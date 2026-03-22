// Effects work together with Protected endpoints.
// Protected must wrap the inner endpoint; Requires wraps Protected.

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_server::auth::Protected;
use typeway_server::effects::EffectfulServer;
use typeway_server::bind_auth;

#[derive(Clone)]
struct AuthUser(u32);
impl FromRequestParts for AuthUser {
    type Error = (http::StatusCode, String);
    fn from_request_parts(_parts: &http::request::Parts) -> Result<Self, Self::Error> {
        Ok(AuthUser(1))
    }
}

typeway_path!(type UsersPath = "users");
typeway_path!(type HealthPath = "health");

// Requires wraps Protected (correct nesting order).
type API = (
    Requires<CorsRequired, GetEndpoint<UsersPath, String>>,
    Protected<AuthUser, GetEndpoint<HealthPath, String>>,
);

async fn get_users() -> &'static str { "users" }
async fn get_health(auth: AuthUser) -> String {
    format!("ok-{}", auth.0)
}

fn main() {
    let _server = EffectfulServer::<API>::new((bind!(get_users), bind_auth!(get_health)))
        .provide::<CorsRequired>()
        .ready();
}
