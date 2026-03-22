// Nested Requires: endpoint requiring multiple effects via nesting.

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_server::effects::EffectfulServer;

typeway_path!(type UsersPath = "users");

// An endpoint that requires both Auth AND CORS.
type API = (
    Requires<AuthRequired, Requires<CorsRequired, GetEndpoint<UsersPath, String>>>,
);

async fn get_users() -> &'static str { "users" }

fn main() {
    // Both effects provided — compiles.
    let _server = EffectfulServer::<API>::new((bind!(get_users),))
        .provide::<AuthRequired>()
        .provide::<CorsRequired>()
        .ready();
}
