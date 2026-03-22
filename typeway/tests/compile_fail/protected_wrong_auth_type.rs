// Error: Protected<AuthUser, E> but handler takes a different auth type.

use typeway::prelude::*;
use typeway_server::auth::Protected;
use typeway_server::bind_auth;

#[derive(Clone)]
struct AuthUser(u32);
impl FromRequestParts for AuthUser {
    type Error = (http::StatusCode, String);
    fn from_request_parts(_parts: &http::request::Parts) -> Result<Self, Self::Error> {
        Ok(AuthUser(1))
    }
}

#[derive(Clone)]
struct AdminUser(u32);
impl FromRequestParts for AdminUser {
    type Error = (http::StatusCode, String);
    fn from_request_parts(_parts: &http::request::Parts) -> Result<Self, Self::Error> {
        Ok(AdminUser(1))
    }
}

typeway_path!(type UsersPath = "users");

// API requires AuthUser...
type API = (Protected<AuthUser, GetEndpoint<UsersPath, String>>,);

// ...but handler takes AdminUser — wrong type.
async fn get_users(_auth: AdminUser) -> &'static str {
    "users"
}

fn main() {
    let _ = Server::<API>::new((bind_auth!(get_users),));
}
