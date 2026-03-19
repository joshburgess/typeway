// Error: using bind!() for a Protected endpoint should fail because
// the handler tuple type doesn't match — BoundHandler<Protected<Auth, E>>
// is not BoundHandler<E>.

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

// Handler doesn't take AuthUser — should not compile for a Protected endpoint
async fn no_auth() -> &'static str {
    "oops"
}

fn main() {
    let _ = Server::<API>::new((bind!(no_auth),));
}
