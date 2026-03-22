// Error: nested Requires with only one of two effects provided.

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_server::effects::EffectfulServer;

typeway_path!(type UsersPath = "users");

type API = (
    Requires<AuthRequired, Requires<CorsRequired, GetEndpoint<UsersPath, String>>>,
);

async fn get_users() -> &'static str { "users" }

fn main() {
    // Only AuthRequired provided, CorsRequired missing — should fail.
    let _server = EffectfulServer::<API>::new((bind!(get_users),))
        .provide::<AuthRequired>()
        .ready();
}
