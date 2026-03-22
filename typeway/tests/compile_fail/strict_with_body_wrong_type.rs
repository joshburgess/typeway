// Error: Strict endpoint where handler returns wrong type with body extractor.

use typeway::prelude::*;
use typeway_server::typed_response::Strict;
use typeway_server::bind_strict;

typeway_path!(type UsersPath = "users");

#[derive(serde::Serialize, serde::Deserialize)]
struct User { name: String }

#[derive(serde::Deserialize)]
struct CreateUser { name: String }

// Strict<PostEndpoint> expects (StatusCode, Json<User>) return type.
type API = (Strict<PostEndpoint<UsersPath, CreateUser, User>>,);

// But handler returns String — wrong.
async fn create_user(body: Json<CreateUser>) -> String {
    body.0.name
}

fn main() {
    let _ = Server::<API>::new((bind_strict!(create_user),));
}
