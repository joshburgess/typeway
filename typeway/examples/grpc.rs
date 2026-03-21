//! Minimal gRPC example — REST and gRPC on the same port.
//!
//! Run with:
//!   cargo run --example grpc --features grpc
//!
//! Test with curl (REST):
//!   curl http://localhost:3000/users
//!   curl -X POST http://localhost:3000/users -H 'Content-Type: application/json' -d '{"name":"Alice"}'
//!
//! Test with grpcurl (gRPC):
//!   grpcurl -plaintext localhost:3000 list
//!   grpcurl -plaintext -d '{"name":"Alice"}' localhost:3000 users.v1.UserService/CreateUser
//!
//! Both protocols share the same handlers — zero duplication.

use typeway::prelude::*;

// 1. Define path types
typeway_path!(type UsersPath = "users");

// 2. Define domain types with protobuf support
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct User {
    id: u32,
    name: String,
}

impl ToProtoType for User {
    fn proto_type_name() -> &'static str { "User" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message User {\n  uint32 id = 1;\n  string name = 2;\n}".to_string())
    }
}

#[derive(Debug, serde::Deserialize)]
struct CreateUser {
    name: String,
}

impl ToProtoType for CreateUser {
    fn proto_type_name() -> &'static str { "CreateUser" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message CreateUser {\n  string name = 1;\n}".to_string())
    }
}

// 3. Define the API as a type
type UserAPI = (
    GetEndpoint<UsersPath, Vec<User>>,
    PostEndpoint<UsersPath, CreateUser, User>,
);

// 4. Write handlers (same handlers serve REST and gRPC)
async fn list_users() -> Json<Vec<User>> {
    Json(vec![
        User { id: 1, name: "Alice".into() },
        User { id: 2, name: "Bob".into() },
    ])
}

async fn create_user(body: Json<CreateUser>) -> (http::StatusCode, Json<User>) {
    let user = User { id: 3, name: body.0.name };
    (http::StatusCode::CREATED, Json(user))
}

// 5. Serve both REST and gRPC
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Server::<UserAPI>::new((
        bind!(list_users),
        bind!(create_user),
    ))
    .with_grpc("UserService", "users.v1")
    .with_grpc_docs()
    .serve("0.0.0.0:3000".parse()?)
    .await
}
