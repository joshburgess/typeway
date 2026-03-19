use typeway_core::*;
use typeway_macros::*;
use typeway_server::*;

// Define path types using the macro — no manual marker types needed.
typeway_path!(type HelloPath = "hello");
typeway_path!(type GreetPath = "greet" / String);

type API = (
    GetEndpoint<HelloPath, String>,
    GetEndpoint<GreetPath, String>,
);

async fn hello() -> &'static str {
    "Hello, world!"
}

async fn greet(path: Path<GreetPath>) -> String {
    let (name,) = path.0;
    format!("Hello, {name}!")
}

#[tokio::main]
async fn main() {
    let server = Server::<API>::new((
        bind::<GetEndpoint<HelloPath, String>, _, _>(hello),
        bind::<GetEndpoint<GreetPath, String>, _, _>(greet),
    ));

    server
        .serve("127.0.0.1:3000".parse().unwrap())
        .await
        .unwrap();
}
