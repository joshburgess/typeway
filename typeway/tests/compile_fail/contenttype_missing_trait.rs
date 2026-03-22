// Error: ContentType<C, E> where C doesn't implement ContentTypeMarker.

use typeway::prelude::*;
use typeway_server::typed::ContentType;

typeway_path!(type UsersPath = "users");

// NotAContentType doesn't implement ContentTypeMarker.
struct NotAContentType;

type API = (ContentType<NotAContentType, GetEndpoint<UsersPath, String>>,);

async fn get_users() -> &'static str { "users" }

fn main() {
    let _ = Server::<API>::new((bind!(get_users),));
}
