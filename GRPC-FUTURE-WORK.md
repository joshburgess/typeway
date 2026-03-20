# typeway-grpc: Future Work

Potential improvements and new features for the gRPC crate, organized by category.

---

## Protocol Improvements

### 1. Full Protobuf Binary Encoding

Currently uses `application/grpc+json` (JSON over gRPC framing). Real gRPC uses `application/grpc` with length-prefixed protobuf binary encoding. Would require `prost` for serialization/deserialization in the bridge and client. This is the biggest gap for interoperability with standard gRPC clients and servers.

### 2. gRPC-Web Support

Browser clients can't do HTTP/2 gRPC natively. gRPC-Web uses HTTP/1.1 with base64 or binary framing. A `GrpcWebLayer` Tower middleware could translate between gRPC-Web and the existing bridge, enabling browser-to-server gRPC without a proxy.

### 3. Client-Streaming and Bidirectional-Streaming RPCs

`ServerStream<E>` handles server→client streaming. Still missing:
- `ClientStream<E>` — client sends a stream of messages, server responds once
- `BidirectionalStream<E>` — full-duplex streaming in both directions

These would need new marker types and corresponding proto output (`stream RequestType` in the RPC definition).

---

## Type-Level Enhancements

### 4. `#[derive(ToProtoType)]` for Enums

Currently only supports structs. Rust enums could map to:
- Simple enums (no data) → protobuf `enum`
- Tagged enums (with data) → protobuf `oneof`

```rust
#[derive(ToProtoType)]
enum Status {
    Active,    // → ACTIVE = 0;
    Inactive,  // → INACTIVE = 1;
    Banned,    // → BANNED = 2;
}

#[derive(ToProtoType)]
enum Payload {
    Text(String),         // → oneof payload { string text = 1; }
    Binary(Vec<u8>),      // →                  bytes binary = 2;
    Structured(UserData), // →                  UserData structured = 3;
}
```

### 5. Proto `map<K,V>` Support

`HashMap<String, T>` → `map<string, T>` in proto. Needs a `ToProtoType` impl for `HashMap` and `BTreeMap` that sets a `is_map` flag, and field rendering that emits `map<key_type, value_type>` syntax.

### 6. Nested Message Flattening in Request Types

When a POST body has nested structs, the proto generator puts the whole thing as one `body` field. Could flatten the body's fields into the request message for a more natural proto API:

```protobuf
// Current:
message CreateUserRequest {
    CreateUser body = 1;
}

// Better:
message CreateUserRequest {
    string name = 1;
    string email = 2;
}
```

### 7. Proto Field Documentation from Doc Comments

`#[derive(ToProtoType)]` could read doc comments and emit them as proto comments:

```rust
#[derive(ToProtoType)]
struct User {
    /// The unique user identifier.
    #[proto(tag = 1)]
    id: u32,
}
```

Generates:
```protobuf
message User {
    // The unique user identifier.
    uint32 id = 1;
}
```

---

## Tooling

### 8. `proto-from-api` with syn Parsing

The CLI command currently prints guidance to use `ApiToProto::to_proto()` programmatically. Could actually parse the Rust source with `syn` (like `typeway-migrate` does) to extract the API type and generate `.proto` files without running the program.

### 9. Proto Validation

Verify the generated `.proto` is valid proto3 syntax:
- Unique field tags within each message
- Valid type names (no Rust-specific types leaking through)
- No reserved words used as field names
- Tag numbers in the valid range (1–536870911, excluding 19000–19999)

### 10. Proto Diff

Compare two generated `.proto` files and report breaking changes:
- Removed fields (breaking)
- Changed field types (breaking)
- Renumbered tags (breaking)
- Added fields (safe)
- Renamed fields (safe in proto3, wire format uses tags not names)

---

## Integration

### 11. Tonic Codegen Bridge

Generate glue code that lets `tonic::include_proto!` types work as typeway handler arguments directly, avoiding dual serialization. The bridge would convert between prost-generated types and serde-based types at the handler boundary.

### 12. gRPC Client Interceptors

Like typeway-client's request/response interceptors but for the `grpc_client!` macro. Auth metadata injection, request logging, retry policies:

```rust
let client = UserServiceClient::new("http://localhost:3000")?
    .with_metadata("authorization", "Bearer token123")
    .with_timeout(Duration::from_secs(5));
```

### 13. Deadline/Timeout Propagation

gRPC deadlines from the `grpc-timeout` header should propagate as a Tower timeout on the REST handler. When a gRPC client sets a 5-second deadline, the REST handler should be cancelled after 5 seconds.

### 14. Structured Error Details

Google's `google.rpc.Status` with a `details` field for structured error payloads beyond just code + message. Enables attaching field-level validation errors, retry info, or debug metadata to gRPC error responses.

---

## Testing

### 15. gRPC Integration Test Helpers

A test client that speaks `grpc+json` for easy assertion in integration tests:

```rust
let test_client = GrpcTestClient::new(addr);
let response = test_client.call("UserService", "GetUser", json!({"id": 42})).await;
assert_eq!(response.grpc_status(), GrpcCode::Ok);
assert_eq!(response.json()["name"], "Alice");
```

This avoids needing a real gRPC client or tonic dependency in test code.
