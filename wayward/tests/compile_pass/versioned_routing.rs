// Versioned<V, E> compiles and routes to /v1/users.

use wayward::prelude::*;
use wayward_server::typed::*;

wayward_path!(type UsersPath = "users");

struct V1;
impl ApiVersion for V1 {
    const PREFIX: &'static str = "v1";
}

type API = (Versioned<V1, GetEndpoint<UsersPath, String>>,);

async fn list_users() -> &'static str {
    "users"
}

fn main() {
    let _ = Server::<API>::new((bind!(list_users),));
}
