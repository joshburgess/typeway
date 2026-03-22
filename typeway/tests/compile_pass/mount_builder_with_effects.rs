// ServerBuilder with .mount() + .provide() compiles when all sub-APIs
// are mounted AND all effects are provided.

use typeway::prelude::*;
use typeway_core::effects::*;

typeway_path!(type UsersPath = "users");
typeway_path!(type ItemsPath = "items");
typeway_path!(type HealthPath = "health");

// UsersAPI requires Auth.
type UsersAPI = (
    Requires<AuthRequired, GetEndpoint<UsersPath, String>>,
    Requires<AuthRequired, PostEndpoint<UsersPath, String, String>>,
);

// ItemsAPI requires Auth AND Cors.
type ItemsAPI = (
    Requires<AuthRequired, Requires<CorsRequired, GetEndpoint<ItemsPath, String>>>,
);

// PublicAPI requires nothing.
type PublicAPI = (
    GetEndpoint<HealthPath, String>,
);

type FullAPI = (UsersAPI, ItemsAPI, PublicAPI);

async fn get_users() -> &'static str { "users" }
async fn create_user(body: Json<String>) -> String { body.0 }
async fn get_items() -> &'static str { "items" }
async fn health() -> &'static str { "ok" }

fn main() {
    // All sub-APIs mounted, all effects provided — compiles.
    let _server = typeway_server::ServerBuilder::<FullAPI>::new()
        .mount::<UsersAPI, _>((bind!(get_users), bind!(create_user)))
        .mount::<ItemsAPI, _>((bind!(get_items),))
        .mount::<PublicAPI, _>((bind!(health),))
        .provide::<AuthRequired>()
        .provide::<CorsRequired>()
        .build();
}
