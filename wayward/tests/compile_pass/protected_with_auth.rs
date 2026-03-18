// Protected<Auth, E> compiles when handler takes Auth as first arg.

use wayward::prelude::*;
use wayward_server::auth::Protected;
use wayward_server::bind_auth;

#[derive(Clone)]
struct AuthUser(u32);

impl FromRequestParts for AuthUser {
    type Error = (http::StatusCode, String);
    fn from_request_parts(_parts: &http::request::Parts) -> Result<Self, Self::Error> {
        Ok(AuthUser(1))
    }
}

wayward_path!(type UserPath = "user");

type API = (Protected<AuthUser, GetEndpoint<UserPath, String>>,);

// Auth as first arg — compiles.
async fn get_user(auth: AuthUser) -> String {
    format!("user-{}", auth.0)
}

fn main() {
    let _ = Server::<API>::new((bind_auth!(get_user),));
}
