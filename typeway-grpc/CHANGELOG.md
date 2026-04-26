# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0

### added:

- **Unified REST + gRPC serving**: `.with_grpc("Service", "package")` on the typeway server enables gRPC alongside REST on the same port, sharing handlers, middleware stack, and runtime
- **`#[derive(ToProtoType)]`**: generate Protocol Buffers message definitions from Rust structs, with `#[proto(tag = N)]` for stable wire format numbering
- **Enum support in `ToProtoType`**: simple enums map to proto `enum` definitions; tagged enums with data map to `oneof` fields
- **`map<K,V>` support**: `HashMap<K,V>` and `BTreeMap<K,V>` field types map to proto map fields
- **Doc comment propagation**: rustdoc comments on structs and fields flow through to generated proto comments
- **Request flattening**: body fields are inlined into the request message rather than wrapped, producing a natural proto API
- **`chrono::DateTime` and `uuid::Uuid` mappings**: enabled via the corresponding feature flags
- **`ApiToProto` trait**: `API::to_proto("Service", "package")` produces a complete `.proto` file from the API type
- **`GrpcReady` compile-time check**: `.with_grpc()` won't compile if any request or response type lacks a `ToProtoType` impl
- **Streaming markers**: `ServerStream<E>`, `ClientStream<E>`, and `BidirectionalStream<E>` use `tokio::sync::mpsc` channels with backpressure
- **`grpc_client!` macro**: derives a typed gRPC client struct from the API type, with codec selection (JSON or binary protobuf)
- **`GrpcClient`**: codec-aware unary and server-streaming client
- **`GrpcClientConfig`**: client interceptors for metadata injection, timeouts, and auth
- **`GrpcClientPool`**: shared HTTP/2 connection pool for multiple clients
- **Server reflection**: `grpc.reflection.v1alpha.ServerReflection` discovery service so tools like `grpcurl` work without a `.proto` on disk
- **Health check**: `grpc.health.v1.Health/Check` with `SERVING` / `NOT_SERVING` transitions for graceful shutdown
- **`GrpcWebLayer`**: Tower middleware translating between gRPC-Web (HTTP/1.1) and native gRPC dispatch for browser clients
- **`IntoGrpcStatus` trait**: error mapping from handler error types to `grpc-status` codes
- **`with_grpc_docs()`**: serves `/grpc-spec` (JSON service spec) and `/grpc-docs` (HTML page)
- **Deadline propagation**: parses `grpc-timeout` header and propagates as a Tower timeout
- **Retry + circuit breaker**: `GrpcRetryPolicy` and `CircuitBreaker` for client-side resilience
- **`validate_proto()` and `diff_protos()`**: proto validation (unique tags, valid identifiers, tag range) and breaking-change detection
- **CLI** (feature `cli`):
  - `proto-from-api`: generate a `.proto` from a typeway API type
  - `api-from-proto`: generate typeway types from a `.proto` file (with optional `--codec` flag for `TypewayCodec` + `BytesStr` output)
  - `diff`: compare two `.proto` files; exits 1 on breaking changes for CI use
- **`proto_to_typeway`** and **`proto_to_typeway_with_codec`**: programmatic equivalents of the CLI codegen modes
- **Proto import resolution**: `--include` / `-I` flag for resolving `import "..."` across multiple directories
