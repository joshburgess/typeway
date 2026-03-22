// All HTTP methods compile as endpoint types in a single API.

use typeway::prelude::*;

typeway_path!(type ItemsPath = "items");
typeway_path!(type ItemPath = "items" / u32);

#[derive(serde::Serialize, serde::Deserialize)]
struct Item { name: String }

type API = (
    GetEndpoint<ItemsPath, Vec<Item>>,
    PostEndpoint<ItemsPath, Item, Item>,
    typeway_core::endpoint::Endpoint<typeway_core::method::Put, ItemPath, Item, Item>,
    typeway_core::endpoint::Endpoint<typeway_core::method::Delete, ItemPath, typeway_core::endpoint::NoBody, String>,
    typeway_core::endpoint::Endpoint<typeway_core::method::Patch, ItemPath, Item, Item>,
);

fn _check() {
    fn _assert_api<T: typeway_core::ApiSpec>() {}
    _assert_api::<API>();
}

fn main() {}
