# Request Validation

Typeway validates request bodies at the type level. `Validated<V, E>`
wraps an endpoint — the validator runs automatically after deserialization,
before the handler.

## Define a validator

Implement the `Validate<T>` trait for your request type:

```rust
use typeway_server::typed::Validate;

#[derive(serde::Deserialize)]
struct CreateUser {
    name: String,
    email: String,
    age: u32,
}

struct CreateUserValidator;

impl Validate<CreateUser> for CreateUserValidator {
    fn validate(body: &CreateUser) -> Result<(), String> {
        if body.name.is_empty() {
            return Err("name is required".into());
        }
        if body.name.len() < 2 {
            return Err("name must be at least 2 characters".into());
        }
        if !body.email.contains('@') {
            return Err("invalid email address".into());
        }
        if body.age > 150 {
            return Err("age must be realistic".into());
        }
        Ok(())
    }
}
```

## Declare validated endpoints

Wrap the endpoint in `Validated<Validator, Endpoint>`:

```rust
use typeway_server::typed::Validated;

type API = (
    GetEndpoint<UsersPath, Vec<User>>,
    Validated<CreateUserValidator, PostEndpoint<UsersPath, CreateUser, User>>,
    Validated<UpdateUserValidator, PutEndpoint<UserByIdPath, UpdateUser, User>>,
);
```

## Bind with `bind_validated!`

```rust
Server::<API>::new((
    bind!(list_users),
    bind_validated!(create_user),   // validator runs before handler
    bind_validated!(update_user),
))
.serve(addr)
.await?;
```

## Error response

When validation fails, the server returns `422 Unprocessable Entity`
with the validation error message:

```json
{
  "error": {
    "status": 422,
    "message": "name must be at least 2 characters"
  }
}
```

## Combining with auth

You can combine `Protected` and `Validated`:

```rust
type API = (
    Protected<AuthUser,
        Validated<CreateUserValidator,
            PostEndpoint<UsersPath, CreateUser, User>
        >
    >,
);
```

Auth runs first (from headers), then validation runs (on the body),
then the handler.

## Handler signature

The handler doesn't change — it receives the already-validated body:

```rust
async fn create_user(body: Json<CreateUser>) -> (http::StatusCode, Json<User>) {
    // body.name is guaranteed non-empty and >= 2 chars
    // body.email is guaranteed to contain @
    let user = User::create(body.0).await;
    (http::StatusCode::CREATED, Json(user))
}
```

No validation logic in the handler. The type system ensures it was
validated before the handler runs.
