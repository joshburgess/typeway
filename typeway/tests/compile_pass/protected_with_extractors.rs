// Protected handler with Auth + additional extractors compiles.

use typeway::prelude::*;
use typeway_server::auth::Protected;
use typeway_server::bind_auth;

#[derive(Clone)]
struct AuthUser { user_id: u32 }

impl FromRequestParts for AuthUser {
    type Error = (http::StatusCode, String);
    fn from_request_parts(_parts: &http::request::Parts) -> Result<Self, Self::Error> {
        Ok(AuthUser { user_id: 1 })
    }
}

#[derive(Clone)]
struct Db;

typeway_path!(type UserPath = "users" / u32);

#[derive(serde::Serialize)]
struct User { id: u32, name: String }

// Auth + Path.
async fn get_user(auth: AuthUser, path: Path<UserPath>) -> Json<User> {
    let (id,) = path.0;
    Json(User { id, name: format!("user-{}", auth.user_id) })
}

// Auth + State.
async fn get_profile(auth: AuthUser, _state: State<Db>) -> String {
    format!("profile-{}", auth.user_id)
}

type API = (
    Protected<AuthUser, GetEndpoint<UserPath, User>>,
    Protected<AuthUser, GetEndpoint<UserPath, String>>,
);

fn main() {
    let _ = Server::<API>::new((
        bind_auth!(get_user),
        bind_auth!(get_profile),
    ));
}
