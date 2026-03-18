// Error: API has 2 endpoints but only 1 handler is provided.
use wayward::prelude::*;

wayward_path!(type HelloPath = "hello");
wayward_path!(type WorldPath = "world");

type API = (
    GetEndpoint<HelloPath, String>,
    GetEndpoint<WorldPath, String>,
);

async fn hello() -> &'static str { "hello" }

fn main() {
    // Only 1 handler for a 2-endpoint API — should fail.
    let _ = Server::<API>::new((
        bind::<_, _, _>(hello),
    ));
}
