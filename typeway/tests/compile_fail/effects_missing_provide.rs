// Error: calling .ready() without providing a required effect.

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_server::effects::EffectfulServer;

typeway_path!(type UsersPath = "users");

type API = (
    Requires<AuthRequired, GetEndpoint<UsersPath, String>>,
);

async fn get_users() -> &'static str { "users" }

fn main() {
    // AuthRequired not provided — should fail.
    let _server = EffectfulServer::<API>::new((bind!(get_users),))
        .ready();
}
