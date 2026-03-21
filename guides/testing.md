# Testing Typeway Applications

Typeway applications are standard Rust async code — test them with
`tokio::test`, `reqwest`, and the built-in `GrpcTestClient`.

## Integration test pattern

Start a server on a random port, make requests, assert responses:

```rust
use std::time::Duration;
use typeway::prelude::*;

// ... define API, types, handlers ...

async fn start_server() -> u16 {
    let server = Server::<API>::new((
        bind!(list_users),
        bind!(create_user),
    ))
    .with_state(initial_state());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        server
            .serve_with_shutdown(listener, std::future::pending())
            .await
            .unwrap();
    });

    // Wait for server to be ready.
    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

#[tokio::test]
async fn list_users_returns_json() {
    let port = start_server().await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let users: Vec<User> = resp.json().await.unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Alice");
}

#[tokio::test]
async fn create_user_returns_201() {
    let port = start_server().await;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/users"))
        .json(&serde_json::json!({"name": "Charlie"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201);
    let user: User = resp.json().await.unwrap();
    assert_eq!(user.name, "Charlie");
}

#[tokio::test]
async fn not_found_returns_404() {
    let port = start_server().await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users/999"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}
```

## Testing gRPC

Use `GrpcTestClient` for gRPC integration tests:

```rust
use typeway_grpc::GrpcTestClient;

#[tokio::test]
async fn grpc_create_user() {
    let port = start_grpc_server().await;

    let client = GrpcTestClient::new(&format!("http://127.0.0.1:{port}"));
    let resp = client
        .call("users.v1.UserService", "CreateUser", serde_json::json!({
            "name": "Charlie"
        }))
        .await;

    assert!(resp.is_ok());
    assert_eq!(resp.json()["name"], "Charlie");
}

#[tokio::test]
async fn grpc_unknown_method_returns_unimplemented() {
    let port = start_grpc_server().await;

    let client = GrpcTestClient::new(&format!("http://127.0.0.1:{port}"));
    let resp = client
        .call_empty("users.v1.UserService", "NonExistent")
        .await;

    assert!(!resp.is_ok());
    assert_eq!(resp.grpc_code(), typeway_grpc::GrpcCode::Unimplemented);
}
```

## Testing streaming

```rust
#[tokio::test]
async fn grpc_streaming() {
    let port = start_grpc_server().await;

    let client = GrpcTestClient::new(&format!("http://127.0.0.1:{port}"));
    let resp = client
        .call_streaming_empty("users.v1.UserService", "ListUser")
        .await;

    assert!(resp.is_ok());
    assert_eq!(resp.len(), 3); // 3 streamed items
    assert_eq!(resp.items[0]["name"], "Alice");
}
```

## Unit testing handlers

Handlers are plain async functions — test them directly:

```rust
#[tokio::test]
async fn handler_returns_correct_user() {
    let user = create_user_logic("Alice").await;
    assert_eq!(user.name, "Alice");
    assert_eq!(user.id, 1);
}
```

## Testing with state

Pass test state to the server:

```rust
async fn start_server_with_state(users: Vec<User>) -> u16 {
    let state = Arc::new(Mutex::new(users));

    let server = Server::<API>::new((
        bind!(list_users),
        bind!(create_user),
    ))
    .with_state(state);

    // ... bind listener and spawn ...
}

#[tokio::test]
async fn empty_state_returns_empty_list() {
    let port = start_server_with_state(vec![]).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/users"))
        .await
        .unwrap();
    let users: Vec<User> = resp.json().await.unwrap();
    assert!(users.is_empty());
}
```

## Feature flags in tests

Gate gRPC tests with `#[cfg]`:

```rust
#![cfg(feature = "grpc")]

#[tokio::test]
async fn grpc_works() {
    // This test only runs with `cargo test --features grpc`
}
```
