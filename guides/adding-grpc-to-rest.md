# Adding gRPC to an Existing REST API

You have a typeway REST API and want to add gRPC. This takes three steps
and zero handler changes.

## Before: REST only

```rust
use typeway::prelude::*;

typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User { id: u32, name: String, email: String }

#[derive(Debug, Deserialize)]
struct CreateUser { name: String, email: String }

type API = (
    GetEndpoint<UsersPath, Vec<User>>,
    GetEndpoint<UserByIdPath, User>,
    PostEndpoint<UsersPath, CreateUser, User>,
);

async fn list_users(state: State<Db>) -> Json<Vec<User>> { /* ... */ }
async fn get_user(path: Path<UserByIdPath>, state: State<Db>) -> Json<User> { /* ... */ }
async fn create_user(state: State<Db>, body: Json<CreateUser>) -> Json<User> { /* ... */ }

Server::<API>::new((
    bind!(list_users),
    bind!(get_user),
    bind!(create_user),
))
.with_state(db)
.serve("0.0.0.0:3000".parse()?)
.await?;
```

## Step 1: Add `ToProtoType` to your types

Tell typeway how to map your Rust types to protobuf messages. The
`#[derive(ToProtoType)]` macro does this automatically:

```rust
use typeway_macros::ToProtoType;

#[derive(Debug, Clone, Serialize, Deserialize, ToProtoType)]
struct User {
    #[proto(tag = 1)]
    id: u32,
    #[proto(tag = 2)]
    name: String,
    #[proto(tag = 3)]
    email: String,
}

#[derive(Debug, Deserialize, ToProtoType)]
struct CreateUser {
    #[proto(tag = 1)]
    name: String,
    #[proto(tag = 2)]
    email: String,
}
```

The `#[proto(tag = N)]` attribute assigns protobuf field numbers. These
must be stable across versions (same rule as `.proto` files).

## Step 2: Add `.with_grpc()`

One line change to the server builder:

```rust
Server::<API>::new((
    bind!(list_users),
    bind!(get_user),
    bind!(create_user),
))
.with_state(db)
.with_grpc("UserService", "users.v1")  // ← add this
.serve("0.0.0.0:3000".parse()?)
.await?;
```

That's it. Your REST handlers now serve gRPC too. The same port serves
both protocols — routing is based on the `Content-Type` header.

## Step 3: Test it

REST still works:
```bash
curl http://localhost:3000/users
```

gRPC works too:
```bash
# List available services
grpcurl -plaintext localhost:3000 list

# Call a method
grpcurl -plaintext -d '{"name":"Alice","email":"alice@example.com"}' \
  localhost:3000 users.v1.UserService/CreateUser
```

## Optional: Add docs and reflection

```rust
.with_grpc("UserService", "users.v1")
.with_grpc_docs()  // serves HTML docs at GET /grpc-docs
```

Server reflection is enabled by default — tools like grpcurl can
discover your API without a `.proto` file.

## Optional: Binary protobuf for standard clients

By default, typeway uses `application/grpc+json` (JSON over gRPC framing).
To support standard gRPC clients that send binary protobuf:

```rust
.with_grpc("UserService", "users.v1")
.with_proto_binary()  // enables binary protobuf transcoding
```

This requires the `grpc-proto-binary` feature flag.

## What you get

From the same API type and the same handlers:

| Projection | What | How |
|-----------|------|-----|
| REST server | HTTP/JSON on `/users`, `/users/:id` | Default |
| gRPC server | HTTP/2 on `/users.v1.UserService/*` | `.with_grpc()` |
| `.proto` file | Generated from your types | `API::to_proto("UserService", "users.v1")` |
| gRPC client | Type-safe client via macro | `grpc_client! { ... }` |
| HTML docs | Service documentation page | `.with_grpc_docs()` |
| Reflection | Runtime service discovery | Enabled by default |
| Health check | `grpc.health.v1.Health/Check` | Enabled by default |

Zero handler duplication. One source of truth.
