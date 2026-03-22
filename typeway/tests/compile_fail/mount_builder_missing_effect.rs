// Error: ServerBuilder fails when a required effect is not provided.

use typeway::prelude::*;
use typeway_core::effects::*;

typeway_path!(type UsersPath = "users");
typeway_path!(type ItemsPath = "items");

type UsersAPI = (Requires<AuthRequired, GetEndpoint<UsersPath, String>>,);
type ItemsAPI = (Requires<CorsRequired, GetEndpoint<ItemsPath, String>>,);
type FullAPI = (UsersAPI, ItemsAPI);

async fn get_users() -> &'static str { "users" }
async fn get_items() -> &'static str { "items" }

fn main() {
    // Both sub-APIs mounted, but only Auth provided — Cors missing.
    let _server = typeway_server::ServerBuilder::<FullAPI>::new()
        .mount::<UsersAPI, _>((bind!(get_users),))
        .mount::<ItemsAPI, _>((bind!(get_items),))
        .provide::<AuthRequired>()
        .build();
}
