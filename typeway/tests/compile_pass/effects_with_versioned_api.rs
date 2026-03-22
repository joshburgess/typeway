// VersionedApi delegates AllProvided to its resolved type.

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_core::versioning::*;
use typeway_server::effects::EffectfulServer;

typeway_path!(type UsersPath = "users");
typeway_path!(type HealthPath = "health");

type V1 = (
    Requires<AuthRequired, GetEndpoint<UsersPath, String>>,
    GetEndpoint<HealthPath, String>,
);

type V2Changes = (
    Added<GetEndpoint<HealthPath, String>>,
);

type V2Resolved = (
    Requires<AuthRequired, GetEndpoint<UsersPath, String>>,
    GetEndpoint<HealthPath, String>,
);

type V2 = VersionedApi<V1, V2Changes, V2Resolved>;

async fn get_users() -> &'static str { "users" }
async fn get_health() -> &'static str { "ok" }

fn main() {
    // VersionedApi delegates AllProvided to V2Resolved — AuthRequired must be provided.
    let _server = EffectfulServer::<V2>::new((bind!(get_users), bind!(get_health)))
        .provide::<AuthRequired>()
        .ready();
}
