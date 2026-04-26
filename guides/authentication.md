# Authentication & Authorization

Typeway enforces authentication at the type level. `Protected<Auth, E>`
wraps an endpoint, the compiler rejects handlers without auth.

## Define an auth extractor

Implement `FromRequestParts` for your auth type. This runs before the
handler and rejects unauthenticated requests:

```rust
use typeway::prelude::*;

#[derive(Clone)]
struct AuthUser {
    username: String,
    role: String,
}

impl FromRequestParts for AuthUser {
    type Error = JsonError;

    fn from_request_parts(parts: &http::request::Parts) -> Result<Self, Self::Error> {
        let token = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| JsonError::unauthorized("missing or invalid token"))?;

        // Validate the token (JWT decode, database lookup, etc.)
        match verify_token(token) {
            Some(user) => Ok(user),
            None => Err(JsonError::unauthorized("invalid token")),
        }
    }
}
```

## Declare protected endpoints

Use `Protected<AuthType, Endpoint>` in your API type:

```rust
use typeway_server::auth::Protected;

type API = (
    // Public, anyone can access
    GetEndpoint<HealthPath, String>,

    // Protected, requires AuthUser
    Protected<AuthUser, GetEndpoint<ProfilePath, UserProfile>>,
    Protected<AuthUser, PostEndpoint<SettingsPath, UpdateSettings, Settings>>,
);
```

## Write handlers

Protected handlers receive the auth type as their first argument:

```rust
async fn get_profile(user: AuthUser, state: State<Db>) -> Json<UserProfile> {
    let profile = state.find_profile(&user.username).await;
    Json(profile)
}

async fn update_settings(
    user: AuthUser,
    state: State<Db>,
    body: Json<UpdateSettings>,
) -> Result<Json<Settings>, JsonError> {
    if user.role != "admin" {
        return Err(JsonError::forbidden("admin access required"));
    }
    let settings = state.update_settings(&user.username, body.0).await;
    Ok(Json(settings))
}
```

## Bind with `bind_auth!`

Protected endpoints use `bind_auth!` instead of `bind!`:

```rust
Server::<API>::new((
    bind!(health_check),           // public, uses bind!
    bind_auth!(get_profile),       // protected, uses bind_auth!
    bind_auth!(update_settings),   // protected, uses bind_auth!
))
.serve(addr)
.await?;
```

The compiler enforces this, using `bind!` on a `Protected` endpoint
is a compile error.

## Role-based access

Check roles inside the handler:

```rust
async fn admin_action(user: AuthUser) -> Result<String, JsonError> {
    if user.role != "admin" {
        return Err(JsonError::forbidden("admin access required"));
    }
    Ok("admin action completed".into())
}
```

Or create a separate auth type for admin:

```rust
struct AdminUser { username: String }

impl FromRequestParts for AdminUser {
    type Error = JsonError;
    fn from_request_parts(parts: &http::request::Parts) -> Result<Self, Self::Error> {
        let user = AuthUser::from_request_parts(parts)?;
        if user.role != "admin" {
            return Err(JsonError::forbidden("admin access required"));
        }
        Ok(AdminUser { username: user.username })
    }
}

// Now the type system enforces admin access:
type AdminAPI = Protected<AdminUser, DeleteEndpoint<UserByIdPath, ()>>;
```

## gRPC compatibility

`Protected` endpoints work with gRPC too. The auth extractor reads
from HTTP headers (metadata in gRPC terms). A gRPC client sends
auth via the `authorization` metadata header, same as REST.
