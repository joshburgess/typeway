// Error: handler with more arguments than the endpoint type expects.

use typeway::prelude::*;

typeway_path!(type ItemsPath = "items");

#[derive(serde::Serialize)]
struct Item { name: String }

#[derive(Clone)]
struct Db;

// This handler takes Path + State + Body, but the endpoint is a GET
// with no body and no path captures — extractor mismatch.
async fn get_items(
    _path: Path<ItemsPath>,
    _state: State<Db>,
    body: Json<Item>,
) -> Json<Vec<Item>> {
    let _ = body;
    Json(vec![])
}

type API = (GetEndpoint<ItemsPath, Vec<Item>>,);

fn main() {
    let _ = Server::<API>::new((bind!(get_items),));
}
