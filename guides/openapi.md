# OpenAPI & Swagger UI

Typeway generates OpenAPI 3.1 specs from your API type automatically.
Add one line to get `/openapi.json` and `/docs` (Swagger UI).

## Basic setup

```rust
Server::<API>::new(handlers)
    .with_openapi("My API", "1.0.0")
    .serve(addr)
    .await?;
```

This serves:
- `GET /openapi.json`, the OpenAPI 3.1 spec
- `GET /docs`, embedded Swagger UI page

## Adding handler documentation

Use the `#[documented_handler]` attribute macro to extract doc comments:

```rust
use typeway_macros::documented_handler;

/// List all users.
///
/// Returns a paginated list of users. Supports filtering by role
/// and sorting by creation date.
#[documented_handler(tags = "users")]
async fn list_users(state: State<Db>) -> Json<Vec<User>> {
    // ...
}

/// Create a new user.
///
/// The username must be unique. Returns 409 Conflict if it already exists.
#[documented_handler(tags = "users")]
async fn create_user(body: Json<CreateUser>) -> (http::StatusCode, Json<User>) {
    // ...
}
```

The macro generates a constant (e.g., `LIST_USERS_DOC`, `CREATE_USER_DOC`)
containing the summary (first line), description (rest), operation ID,
and tags.

Pass these to the server:

```rust
Server::<API>::new(handlers)
    .with_openapi_docs("My API", "1.0.0", &handler_docs![
        LIST_USERS_DOC,
        CREATE_USER_DOC,
    ])
    .serve(addr)
    .await?;
```

## What gets generated

The spec includes:
- Paths derived from your endpoint types
- Request/response schemas from `serde` types
- HTTP methods from `GetEndpoint`, `PostEndpoint`, etc.
- Path parameters from `Capture<T>` segments
- Query parameters from `Query<T>` types
- Tags, summaries, and descriptions from `#[documented_handler]`

## Schema generation

Types that implement `serde::Serialize` + `serde::Deserialize` are
automatically converted to JSON Schema in the OpenAPI spec. For
custom schema control, implement the `ToSchema` trait:

```rust
use typeway_openapi::ToSchema;

impl ToSchema for User {
    fn schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer", "format": "int32" },
                "name": { "type": "string", "minLength": 1 },
                "email": { "type": "string", "format": "email" }
            },
            "required": ["id", "name", "email"]
        })
    }
}
```

## Security schemes

Add authentication documentation:

```rust
use typeway_openapi::{SecurityScheme, SecurityRequirement};

let spec = API::to_spec("My API", "1.0.0");
// Add bearer auth scheme to spec...
```

## Combining with gRPC

OpenAPI and gRPC work on the same server:

```rust
Server::<API>::new(handlers)
    .with_openapi_docs("My API", "1.0.0", &docs)
    .with_grpc("MyService", "my.v1")
    .with_grpc_docs()
    .serve(addr)
    .await?;
```

Now you have:
- `/docs`. Swagger UI for REST
- `/openapi.json`. OpenAPI spec
- `/grpc-docs`, gRPC service documentation
- `/grpc-spec`, gRPC service spec (JSON)
