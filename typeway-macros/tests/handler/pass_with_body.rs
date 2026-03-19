use typeway_macros::handler;
use typeway_server::Json;

#[derive(serde::Deserialize)]
struct CreateUser {
    name: String,
}

#[derive(serde::Serialize)]
struct User {
    name: String,
}

#[handler]
async fn create_user(body: Json<CreateUser>) -> Json<User> {
    Json(User { name: body.0.name })
}

fn main() {}
