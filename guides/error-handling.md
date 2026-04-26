# Error Handling

Typeway provides `JsonError` for structured error responses and
`Result<T, E>` return types for handlers.

## Basic pattern

Return `Result<T, JsonError>` from your handler:

```rust
use typeway::prelude::*;

async fn get_user(
    path: Path<UserByIdPath>,
    state: State<Db>,
) -> Result<Json<User>, JsonError> {
    let (id,) = path.0;
    state.find_user(id)
        .await
        .map(Json)
        .ok_or_else(|| JsonError::not_found(format!("user {id} not found")))
}
```

## JsonError constructors

| Constructor | HTTP Status | When to use |
|------------|-------------|-------------|
| `JsonError::bad_request(msg)` | 400 | Malformed request |
| `JsonError::unauthorized(msg)` | 401 | Missing or invalid credentials |
| `JsonError::forbidden(msg)` | 403 | Valid credentials, insufficient permissions |
| `JsonError::not_found(msg)` | 404 | Resource doesn't exist |
| `JsonError::conflict(msg)` | 409 | Resource already exists |
| `JsonError::unprocessable(msg)` | 422 | Validation failure |
| `JsonError::internal(msg)` | 500 | Server error |
| `JsonError::new(status, msg)` | Custom | Any HTTP status |

## Error response format

All errors serialize as:

```json
{
  "error": {
    "status": 404,
    "message": "user 42 not found"
  }
}
```

## Using Result with StatusCode

For simple cases, return `Result<T, StatusCode>`:

```rust
async fn get_user(path: Path<UserByIdPath>) -> Result<Json<User>, http::StatusCode> {
    find_user(path.0.0)
        .map(Json)
        .ok_or(http::StatusCode::NOT_FOUND)
}
```

This returns the status code with an empty body.

## Custom error types

Implement `IntoResponse` for your error type:

```rust
#[derive(Debug)]
enum AppError {
    NotFound(String),
    Forbidden,
    Database(sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> http::Response<BoxBody> {
        match self {
            AppError::NotFound(msg) => JsonError::not_found(msg).into_response(),
            AppError::Forbidden => JsonError::forbidden("access denied").into_response(),
            AppError::Database(e) => {
                tracing::error!("database error: {e}");
                JsonError::internal("internal server error").into_response()
            }
        }
    }
}

async fn handler() -> Result<Json<Data>, AppError> {
    let data = db.query().await.map_err(AppError::Database)?;
    Ok(Json(data))
}
```

## Error handling with gRPC

`JsonError` maps to gRPC status codes automatically:

| HTTP Status | gRPC Code |
|-------------|-----------|
| 400 | INVALID_ARGUMENT |
| 401 | UNAUTHENTICATED |
| 403 | PERMISSION_DENIED |
| 404 | NOT_FOUND |
| 409 | ALREADY_EXISTS |
| 422 | INVALID_ARGUMENT |
| 500 | INTERNAL |

When a handler returns `Err(JsonError::not_found(...))`, REST clients
get `404` and gRPC clients get `grpc-status: 5` (NOT_FOUND), from
the same handler.

## Tuple responses

Return a status code with a body:

```rust
async fn create_user(body: Json<CreateUser>) -> (http::StatusCode, Json<User>) {
    let user = User::create(body.0).await;
    (http::StatusCode::CREATED, Json(user))
}
```

The tuple `(StatusCode, T)` sets the status and serializes `T` as the body.
