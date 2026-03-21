# Working with Standard gRPC Clients

By default, typeway uses `application/grpc+json` — JSON payloads over
gRPC framing. This works great for typeway-to-typeway communication but
standard gRPC tools (grpcurl, Postman, tonic clients, Go/Python/Java
clients) expect binary protobuf (`application/grpc`).

This guide shows how to enable binary protobuf interop.

## Enable binary protobuf

Add the feature flag to your `Cargo.toml`:

```toml
[dependencies]
typeway = { version = "0.1", features = ["grpc"] }

[features]
grpc-proto-binary = ["typeway/grpc-proto-binary"]  # or however your project structures this
```

Then call `.with_proto_binary()` on the server builder:

```rust
Server::<API>::new(handlers)
    .with_state(state)
    .with_grpc("UserService", "users.v1")
    .with_proto_binary()  // ← enables binary protobuf
    .with_grpc_docs()
    .serve(addr)
    .await?;
```

The server now accepts both formats:
- `application/grpc+json` — JSON (typeway clients, debugging)
- `application/grpc` — binary protobuf (standard clients)

The response format mirrors the request: binary clients get binary
responses, JSON clients get JSON responses.

## Test with grpcurl

```bash
# List available services (uses reflection, no .proto needed)
grpcurl -plaintext localhost:3000 list

# Describe a service
grpcurl -plaintext localhost:3000 describe users.v1.UserService

# Call a method with JSON input (grpcurl transcodes to binary)
grpcurl -plaintext \
  -d '{"name": "Alice", "email": "alice@example.com"}' \
  localhost:3000 users.v1.UserService/CreateUser
```

## Test with Postman

1. Import the service via server reflection (Postman supports this)
2. Set the server URL to `localhost:3000`
3. Select the method and fill in the request fields
4. Postman sends binary protobuf automatically

## Connect from a tonic client

```rust
use tonic::transport::Channel;

let channel = Channel::from_static("http://localhost:3000")
    .connect()
    .await?;

// Use your tonic-generated client as normal.
// The typeway server speaks standard gRPC binary on the wire.
let mut client = UserServiceClient::new(channel);
let response = client.create_user(CreateUserRequest {
    name: "Alice".into(),
    email: "alice@example.com".into(),
}).await?;
```

## Generate a .proto file

If your clients need a `.proto` file (for code generation in Go, Python,
Java, etc.):

```rust
let proto = API::to_proto("UserService", "users.v1");
std::fs::write("proto/users.proto", proto)?;
```

Or use the CLI:

```bash
cargo run -p typeway-grpc --features cli -- proto-from-api \
  --service UserService --package users.v1
```

The generated `.proto` file is standard protobuf3 and works with
`protoc`, `buf`, or any language's protobuf toolchain.

## How it works

When `.with_proto_binary()` is enabled, the server:

1. Detects the request content type (`application/grpc` vs `application/grpc+json`)
2. For binary requests: decodes protobuf binary → JSON for the handler
3. Handler runs (same handler, same logic)
4. For binary responses: encodes JSON → protobuf binary for the client

This transcoding is transparent. Your handlers don't change.

For maximum performance with binary clients, use `Proto<T>` instead of
`Json<T>` as your extractor — it decodes binary protobuf directly via
`TypewayDecode` without the JSON intermediate step.
