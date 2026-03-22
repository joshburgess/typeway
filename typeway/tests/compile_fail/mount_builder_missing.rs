// Error: ServerBuilder fails when a sub-API is not mounted.

use typeway::prelude::*;

typeway_path!(type UsersPath = "users");
typeway_path!(type ItemsPath = "items");

type UsersAPI = (GetEndpoint<UsersPath, String>,);
type ItemsAPI = (GetEndpoint<ItemsPath, String>,);
type FullAPI = (UsersAPI, ItemsAPI);

async fn get_users() -> &'static str { "users" }

fn main() {
    // Only UsersAPI mounted, ItemsAPI missing — should fail.
    let _server = typeway_server::ServerBuilder::<FullAPI>::new()
        .mount::<UsersAPI, _>((bind!(get_users),))
        .build();
}
