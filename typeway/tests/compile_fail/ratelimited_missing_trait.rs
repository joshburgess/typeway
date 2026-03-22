// Error: RateLimited<R, E> where R doesn't implement RateLimit.

use typeway::prelude::*;
use typeway_server::typed::RateLimited;

typeway_path!(type UsersPath = "users");

// NotARateLimit doesn't implement the RateLimit trait.
struct NotARateLimit;

type API = (RateLimited<NotARateLimit, GetEndpoint<UsersPath, String>>,);

async fn get_users() -> &'static str { "users" }

fn main() {
    let _ = Server::<API>::new((bind!(get_users),));
}
