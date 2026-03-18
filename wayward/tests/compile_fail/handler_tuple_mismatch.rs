// Error: handler tuple has wrong number of elements for the API.
use wayward::prelude::*;

wayward_path!(type HelloPath = "hello");

type API = (GetEndpoint<HelloPath, String>,);

async fn hello() -> &'static str { "hello" }
async fn world() -> &'static str { "world" }

fn main() {
    // 2 handlers for a 1-endpoint API — should fail.
    let _ = Server::<API>::new((
        bind::<_, _, _>(hello),
        bind::<_, _, _>(world),
    ));
}
