// Effects system: EffectfulServer compiles when all effects are provided.

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_server::effects::EffectfulServer;

typeway_path!(type UsersPath = "users");
typeway_path!(type HealthPath = "health");

type API = (
    Requires<AuthRequired, GetEndpoint<UsersPath, String>>,
    Requires<CorsRequired, GetEndpoint<HealthPath, String>>,
);

async fn get_users() -> &'static str { "users" }
async fn get_health() -> &'static str { "ok" }

fn main() {
    // .ready() only compiles when all effects are provided.
    let _server = EffectfulServer::<API>::new((bind!(get_users), bind!(get_health)))
        .provide::<AuthRequired>()
        .provide::<CorsRequired>()
        .ready();
}
