// APIs larger than 22 endpoints can use SubApi to nest sub-APIs.

use typeway::prelude::*;

typeway_path!(type UsersPath = "users");
typeway_path!(type ItemsPath = "items");
typeway_path!(type TagsPath = "tags");
typeway_path!(type NotesPath = "notes");

// Two sub-APIs.
type UsersAPI = (
    GetEndpoint<UsersPath, String>,
    PostEndpoint<UsersPath, String, String>,
);

type ItemsAPI = (
    GetEndpoint<ItemsPath, String>,
    GetEndpoint<TagsPath, String>,
    GetEndpoint<NotesPath, String>,
);

// Composed API — nested tuples are valid ApiSpec.
type FullAPI = (UsersAPI, ItemsAPI);

fn _check() {
    fn _assert_api<T: typeway_core::ApiSpec>() {}
    _assert_api::<FullAPI>();
}

// Handlers.
async fn get_users() -> &'static str { "users" }
async fn create_user(body: Json<String>) -> String { body.0 }
async fn get_items() -> &'static str { "items" }
async fn get_tags() -> &'static str { "tags" }
async fn get_notes() -> &'static str { "notes" }

fn main() {
    // SubApi wraps each sub-API's handlers.
    let _ = Server::<FullAPI>::new((
        typeway_server::SubApi::<UsersAPI, _>::new((bind!(get_users), bind!(create_user))),
        typeway_server::SubApi::<ItemsAPI, _>::new((bind!(get_items), bind!(get_tags), bind!(get_notes))),
    ));
}
