// An API with multiple endpoints and methods compiles.
use wayward::prelude::*;

wayward_path!(type UsersPath = "users");
wayward_path!(type UserByIdPath = "users" / u32);

#[derive(serde::Serialize, serde::Deserialize)]
struct User {
    id: u32,
}

type API = (
    GetEndpoint<UsersPath, Vec<User>>,
    GetEndpoint<UserByIdPath, User>,
    PostEndpoint<UsersPath, User, User>,
    DeleteEndpoint<UserByIdPath, ()>,
);

async fn list() -> Json<Vec<User>> {
    Json(vec![])
}
async fn get(path: Path<UserByIdPath>) -> Json<User> {
    let (id,) = path.0;
    Json(User { id })
}
async fn create(body: Json<User>) -> Json<User> {
    body
}
async fn delete(_path: Path<UserByIdPath>) -> http::StatusCode {
    http::StatusCode::NO_CONTENT
}

fn main() {
    let _ = Server::<API>::new((
        bind::<_, _, _>(list),
        bind::<_, _, _>(get),
        bind::<_, _, _>(create),
        bind::<_, _, _>(delete),
    ));
}
