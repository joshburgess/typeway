// Providing extra effects beyond what the API requires is fine.

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_server::effects::EffectfulServer;

typeway_path!(type UsersPath = "users");

type API = (
    Requires<AuthRequired, GetEndpoint<UsersPath, String>>,
);

async fn get_users() -> &'static str { "users" }

fn main() {
    // AuthRequired is required; CorsRequired and TracingRequired are extra — compiles.
    let _server = EffectfulServer::<API>::new((bind!(get_users),))
        .provide::<AuthRequired>()
        .provide::<CorsRequired>()
        .provide::<TracingRequired>()
        .ready();
}
