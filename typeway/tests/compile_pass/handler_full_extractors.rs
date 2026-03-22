// Handler with Path + State + Json body (all three) compiles.

use typeway::prelude::*;

typeway_path!(type UserPath = "users" / u32);

#[derive(Clone)]
struct Db;

#[derive(serde::Serialize, serde::Deserialize)]
struct User { id: u32, name: String }

#[derive(serde::Deserialize)]
struct UpdateUser { name: String }

// Path + State + Body — all extractors together.
async fn update_user(
    path: Path<UserPath>,
    _state: State<Db>,
    body: Json<UpdateUser>,
) -> Json<User> {
    let (id,) = path.0;
    Json(User { id, name: body.0.name })
}

type API = (
    PostEndpoint<UserPath, UpdateUser, User>,
);

fn main() {
    let _ = Server::<API>::new((bind!(update_user),));
}
