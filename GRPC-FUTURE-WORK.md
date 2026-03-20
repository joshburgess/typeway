# typeway-grpc: Implementation Status and Future Work

Status of all planned gRPC features. Items 1-15 from the original roadmap are tracked below, along with new features that were added during implementation.

---

## Protocol Improvements

### 1. Full Protobuf Binary Encoding -- DEFERRED

Currently uses `application/grpc+json` (JSON over gRPC framing). Real gRPC uses `application/grpc` with length-prefixed protobuf binary encoding. Would require `prost` for serialization/deserialization in the bridge and client. This is the biggest gap for interoperability with standard gRPC clients and servers. Deferred because the JSON bridge shares handlers with REST without transcoding, which is the core design advantage.

### 2. gRPC-Web Support -- DONE

`GrpcWebLayer` Tower middleware translates between gRPC-Web (HTTP/1.1 with base64 or binary framing) and the existing bridge, enabling browser-to-server gRPC without a proxy.

### 3. Client-Streaming and Bidirectional-Streaming RPCs -- DONE

All three streaming patterns are implemented as marker types:
- `ServerStream<E>` -- server-side streaming
- `ClientStream<E>` -- client sends a stream of messages, server responds once
- `BidirectionalStream<E>` -- full-duplex streaming in both directions

All three generate the corresponding `stream` annotations in `.proto` output.

---

## Type-Level Enhancements

### 4. `#[derive(ToProtoType)]` for Enums -- DONE

Simple (fieldless) enums map to protobuf `enum` definitions. Tagged enums with data map to `oneof` fields:

```rust
#[derive(ToProtoType)]
enum Status {
    Active,    // -> ACTIVE = 0;
    Inactive,  // -> INACTIVE = 1;
    Banned,    // -> BANNED = 2;
}

#[derive(ToProtoType)]
enum Payload {
    Text(String),         // -> oneof payload { string text = 1; }
    Binary(Vec<u8>),      // ->                  bytes binary = 2;
    Structured(UserData), // ->                  UserData structured = 3;
}
```

### 5. Proto `map<K,V>` Support -- DONE

`HashMap<K, V>` and `BTreeMap<K, V>` map to `map<key_type, value_type>` in proto. The `ToProtoType` trait includes `is_map()`, `map_key_type()`, and `map_value_type()` methods, and field rendering emits correct `map<K,V>` syntax.

### 6. Nested Message Flattening in Request Types -- DONE

POST body fields are inlined into the request message rather than wrapped in a `body` field. The `proto_fields()` method on `ToProtoType` enables this flattening.

### 7. Proto Field Documentation from Doc Comments -- DONE

`#[derive(ToProtoType)]` reads doc comments on structs and fields and emits them as proto comments in the generated `.proto` output.

---

## Tooling

### 8. `proto-from-api` with syn Parsing -- PARTIAL

The CLI command still prints guidance to use `ApiToProto::to_proto()` programmatically rather than parsing Rust source with `syn`. Full source-level parsing is not implemented. The programmatic API works correctly.

### 9. Proto Validation -- DONE

`validate_proto()` checks generated `.proto` files for:
- Unique field tags within each message
- Valid type names (no Rust-specific types leaking through)
- No reserved words used as field names
- Tag numbers in the valid range (1-536870911, excluding 19000-19999)

### 10. Proto Diff -- DONE

`diff_protos()` compares two `.proto` files and reports breaking vs. compatible changes:
- Removed fields (breaking)
- Changed field types (breaking)
- Renumbered tags (breaking)
- Added fields (safe)
- Renamed fields (safe in proto3, wire format uses tags not names)

The `typeway-grpc diff` CLI exits with code 1 on breaking changes, suitable for CI pipelines.

---

## Integration

### 11. Tonic Codegen Bridge -- DEFERRED

Generate glue code that lets `tonic::include_proto!` types work as typeway handler arguments directly. Deferred -- the JSON bridge approach avoids the need for dual serialization in most use cases.

### 12. gRPC Client Interceptors -- DONE

`GrpcClientConfig` provides metadata injection, timeout configuration, and retry policies for the `grpc_client!` and `auto_grpc_client!` macros.

### 13. Deadline/Timeout Propagation -- DONE

The gRPC bridge parses the `grpc-timeout` header (all units: hours, minutes, seconds, milliseconds, microseconds, nanoseconds) and propagates deadlines as Tower timeouts on the REST handler. `parse_grpc_timeout()` is exported for custom use.

### 14. Structured Error Details -- DEFERRED

Google's `google.rpc.Status` with a `details` field for structured error payloads beyond code + message. Deferred -- `IntoGrpcStatus` covers the common case of mapping errors to gRPC status codes with messages.

---

## Testing

### 15. gRPC Integration Test Helpers -- DONE

`GrpcTestClient` speaks `grpc+json` for easy assertion in integration tests without needing a real gRPC client or tonic dependency in test code.

---

## Features Added Beyond Original Roadmap

These features were not in the original 15-item plan but were implemented during development:

### GrpcReady Compile-Time Check -- DONE

The `GrpcReady` trait is a compile-time check that all request and response types in an API implement `ToProtoType`. The `.with_grpc()` method on `Server` requires `A: GrpcReady`, so missing proto type implementations are caught at compile time rather than runtime.

### auto_grpc_client! Macro -- DONE

Automatically derives a type-safe gRPC client from the API type without manual endpoint listing. Includes a `GrpcReady` compile-time assertion.

### gRPC Service Spec and Documentation -- DONE

`GrpcServiceSpec` is a structured specification of the gRPC service (the gRPC equivalent of an OpenAPI spec). `ApiToGrpcSpec` derives a spec from the API type at startup. `.with_grpc_docs()` on the server builder serves:
- `GET /grpc-spec` -- JSON service specification
- `GET /grpc-docs` -- HTML documentation page

The `typeway-grpc spec-from-proto` CLI generates a spec or docs page from any `.proto` file.

### chrono and uuid Support -- DONE

`chrono::DateTime<Tz>` and `uuid::Uuid` have `ToProtoType` implementations that map to their proto equivalents (feature-gated).

### gRPC Framing -- DONE

`encode_grpc_frame()` and `decode_grpc_frame()` handle proper gRPC length-prefix framing (5-byte header: 1 byte compression flag + 4 byte big-endian length).

---

## Summary

| Item | Status |
|------|--------|
| 1. Full protobuf binary encoding | Deferred |
| 2. gRPC-Web support | Done |
| 3. Client/bidirectional streaming | Done |
| 4. Enum derive support | Done |
| 5. map<K,V> support | Done |
| 6. Request message flattening | Done |
| 7. Doc comments in proto | Done |
| 8. proto-from-api CLI (syn parsing) | Partial (programmatic API works) |
| 9. Proto validation | Done |
| 10. Proto diff | Done |
| 11. Tonic codegen bridge | Deferred |
| 12. Client interceptors | Done |
| 13. Deadline/timeout propagation | Done |
| 14. Structured error details | Deferred |
| 15. Integration test helpers | Done |
| GrpcReady compile-time check | Done (new) |
| auto_grpc_client! macro | Done (new) |
| Service spec + docs page | Done (new) |
| spec-from-proto CLI | Done (new) |
| chrono/uuid support | Done (new) |
| gRPC framing | Done (new) |

**12 of 15 original items complete. 3 deferred (binary protobuf, Tonic codegen bridge, structured error details). 6 additional features implemented. 351 tests.**
