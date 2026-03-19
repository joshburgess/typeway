//! Full CRUD API with structured error handling.
//!
//! Run: cargo run -p typeway-server --example crud
//! Test:
//!   curl http://127.0.0.1:3000/items                           # list
//!   curl http://127.0.0.1:3000/items/1                         # get
//!   curl -X POST http://127.0.0.1:3000/items \
//!        -H 'Content-Type: application/json' \
//!        -d '{"name":"Widget","price":9.99}'                    # create
//!   curl -X PUT http://127.0.0.1:3000/items/1 \
//!        -H 'Content-Type: application/json' \
//!        -d '{"name":"Updated Widget","price":19.99}'           # update
//!   curl -X DELETE http://127.0.0.1:3000/items/1               # delete

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use typeway_core::*;
use typeway_macros::*;
use typeway_server::*;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Item {
    id: u32,
    name: String,
    price: f64,
}

#[derive(Debug, Deserialize)]
struct CreateItem {
    name: String,
    price: f64,
}

#[derive(Debug, Deserialize)]
struct UpdateItem {
    name: Option<String>,
    price: Option<f64>,
}

type Db = Arc<Mutex<Vec<Item>>>;

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

typeway_path!(type ItemsPath = "items");
typeway_path!(type ItemByIdPath = "items" / u32);

type API = (
    GetEndpoint<ItemsPath, Vec<Item>>,
    GetEndpoint<ItemByIdPath, Item>,
    PostEndpoint<ItemsPath, CreateItem, Item>,
    PutEndpoint<ItemByIdPath, UpdateItem, Item>,
    DeleteEndpoint<ItemByIdPath, ()>,
);

// ---------------------------------------------------------------------------
// Handlers — all return Result<T, JsonError> for structured errors
// ---------------------------------------------------------------------------

async fn list_items(state: State<Db>) -> Json<Vec<Item>> {
    Json(state.0.lock().unwrap().clone())
}

async fn get_item(path: Path<ItemByIdPath>, state: State<Db>) -> Result<Json<Item>, JsonError> {
    let (id,) = path.0;
    let items = state.0.lock().unwrap();
    items
        .iter()
        .find(|i| i.id == id)
        .cloned()
        .map(Json)
        .ok_or_else(|| JsonError::not_found(format!("item {id} not found")))
}

async fn create_item(
    state: State<Db>,
    body: Json<CreateItem>,
) -> Result<(http::StatusCode, Json<Item>), JsonError> {
    if body.0.name.is_empty() {
        return Err(JsonError::bad_request("name cannot be empty"));
    }
    if body.0.price < 0.0 {
        return Err(JsonError::unprocessable("price must be non-negative"));
    }

    let mut items = state.0.lock().unwrap();
    let id = items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
    let item = Item {
        id,
        name: body.0.name,
        price: body.0.price,
    };
    items.push(item.clone());
    Ok((http::StatusCode::CREATED, Json(item)))
}

async fn update_item(
    path: Path<ItemByIdPath>,
    state: State<Db>,
    body: Json<UpdateItem>,
) -> Result<Json<Item>, JsonError> {
    let (id,) = path.0;
    let mut items = state.0.lock().unwrap();
    let item = items
        .iter_mut()
        .find(|i| i.id == id)
        .ok_or_else(|| JsonError::not_found(format!("item {id} not found")))?;

    if let Some(name) = &body.0.name {
        if name.is_empty() {
            return Err(JsonError::bad_request("name cannot be empty"));
        }
        item.name = name.clone();
    }
    if let Some(price) = body.0.price {
        if price < 0.0 {
            return Err(JsonError::unprocessable("price must be non-negative"));
        }
        item.price = price;
    }

    Ok(Json(item.clone()))
}

async fn delete_item(
    path: Path<ItemByIdPath>,
    state: State<Db>,
) -> Result<http::StatusCode, JsonError> {
    let (id,) = path.0;
    let mut items = state.0.lock().unwrap();
    let pos = items
        .iter()
        .position(|i| i.id == id)
        .ok_or_else(|| JsonError::not_found(format!("item {id} not found")))?;
    items.remove(pos);
    Ok(http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let db: Db = Arc::new(Mutex::new(vec![
        Item {
            id: 1,
            name: "Hammer".into(),
            price: 12.99,
        },
        Item {
            id: 2,
            name: "Screwdriver".into(),
            price: 7.49,
        },
    ]));

    let server = Server::<API>::new((
        bind!(list_items),
        bind!(get_item),
        bind!(create_item),
        bind!(update_item),
        bind!(delete_item),
    ))
    .with_state(db)
    .max_body_size(1024 * 1024); // 1 MiB

    println!("CRUD example on http://127.0.0.1:3000");
    println!("  GET    /items      — list all");
    println!("  GET    /items/:id  — get one");
    println!("  POST   /items      — create");
    println!("  PUT    /items/:id  — update");
    println!("  DELETE /items/:id  — delete");

    server
        .serve("127.0.0.1:3000".parse().unwrap())
        .await
        .unwrap();
}
