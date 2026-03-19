// Error: using bind!() on a Protected endpoint should fail.
// Must use bind_auth!() instead.

use typeway::prelude::*;
use typeway_server::auth::Protected;

#[derive(Clone)]
struct AuthUser;

impl FromRequestParts for AuthUser {
    type Error = (http::StatusCode, String);
    fn from_request_parts(_parts: &http::request::Parts) -> Result<Self, Self::Error> {
        Ok(AuthUser)
    }
}

typeway_path!(type HelloPath = "hello");

type API = (Protected<AuthUser, GetEndpoint<HelloPath, String>>,);

async fn handler() -> &'static str {
    "no auth"
}

fn main() {
    // bind!() should fail for Protected — must use bind_auth!()
    let _ = Server::<API>::new((bind!(handler),));
}
