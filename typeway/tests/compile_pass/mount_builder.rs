// ServerBuilder with .mount() compiles when all sub-APIs are mounted.

use typeway::prelude::*;

typeway_path!(type UsersPath = "users");
typeway_path!(type ItemsPath = "items");
typeway_path!(type TagsPath = "tags");

type UsersAPI = (
    GetEndpoint<UsersPath, String>,
    PostEndpoint<UsersPath, String, String>,
);

type ItemsAPI = (
    GetEndpoint<ItemsPath, String>,
    GetEndpoint<TagsPath, String>,
);

type FullAPI = (UsersAPI, ItemsAPI);

async fn get_users() -> &'static str { "users" }
async fn create_user(body: Json<String>) -> String { body.0 }
async fn get_items() -> &'static str { "items" }
async fn get_tags() -> &'static str { "tags" }

fn main() {
    // Builder style: flat, order-independent, type-checked.
    let _server = typeway_server::ServerBuilder::<FullAPI>::new()
        .mount::<UsersAPI, _>((bind!(get_users), bind!(create_user)))
        .mount::<ItemsAPI, _>((bind!(get_items), bind!(get_tags)))
        .build();
}
