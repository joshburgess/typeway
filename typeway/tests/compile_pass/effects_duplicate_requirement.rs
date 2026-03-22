// Multiple endpoints requiring the same effect only need one provide().

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_server::effects::EffectfulServer;

typeway_path!(type UsersPath = "users");
typeway_path!(type ItemsPath = "items");

type API = (
    Requires<AuthRequired, GetEndpoint<UsersPath, String>>,
    Requires<AuthRequired, GetEndpoint<ItemsPath, String>>,
);

async fn get_users() -> &'static str { "users" }
async fn get_items() -> &'static str { "items" }

fn main() {
    // One provide::<AuthRequired>() covers both endpoints.
    let _server = EffectfulServer::<API>::new((bind!(get_users), bind!(get_items)))
        .provide::<AuthRequired>()
        .ready();
}
