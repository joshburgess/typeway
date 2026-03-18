// A basic wayward server compiles successfully.
use wayward::prelude::*;

wayward_path!(type HelloPath = "hello");

type API = (GetEndpoint<HelloPath, String>,);

async fn hello() -> &'static str {
    "hello"
}

fn main() {
    let _ = Server::<API>::new((bind::<_, _, _>(hello),));
}
