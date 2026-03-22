// API with no Requires wrappers compiles without any provide() calls.

use typeway::prelude::*;
use typeway_server::effects::EffectfulServer;

typeway_path!(type UsersPath = "users");

type API = (GetEndpoint<UsersPath, String>,);

async fn get_users() -> &'static str { "users" }

fn main() {
    // No Requires in API — .ready() compiles with no .provide() calls.
    let _server = EffectfulServer::<API>::new((bind!(get_users),))
        .ready();
}
