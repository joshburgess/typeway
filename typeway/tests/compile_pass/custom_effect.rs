// Custom user-defined Effect type works with EffectfulServer.

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_server::effects::EffectfulServer;

// Custom effect.
struct MetricsRequired;
impl Effect for MetricsRequired {}

typeway_path!(type UsersPath = "users");
typeway_path!(type HealthPath = "health");

type API = (
    Requires<MetricsRequired, GetEndpoint<UsersPath, String>>,
    Requires<AuthRequired, GetEndpoint<HealthPath, String>>,
);

async fn get_users() -> &'static str { "users" }
async fn get_health() -> &'static str { "ok" }

fn main() {
    // Custom + built-in effects both provided — compiles.
    let _server = EffectfulServer::<API>::new((bind!(get_users), bind!(get_health)))
        .provide::<MetricsRequired>()
        .provide::<AuthRequired>()
        .ready();
}
