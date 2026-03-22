// Error: Versioned<V, E> where V doesn't implement ApiVersion.

use typeway::prelude::*;
use typeway_server::typed::Versioned;

typeway_path!(type UsersPath = "users");

// NotAVersion doesn't implement ApiVersion.
struct NotAVersion;

type API = (Versioned<NotAVersion, GetEndpoint<UsersPath, String>>,);

async fn get_users() -> &'static str { "users" }

fn main() {
    let _ = Server::<API>::new((bind!(get_users),));
}
