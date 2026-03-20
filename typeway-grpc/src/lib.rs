//! `typeway-grpc` — gRPC / Protocol Buffers interop for the Typeway web framework.
//!
//! This crate provides two layers of gRPC support:
//!
//! 1. **Proto generation** — given an API type (a tuple of endpoints),
//!    [`ApiToProto::to_proto`] produces a complete `.proto` file with service
//!    and message definitions.
//!
//! 2. **gRPC bridge** — [`GrpcBridge`](bridge::GrpcBridge) is a Tower service
//!    that translates incoming gRPC requests into REST requests and forwards
//!    them to the typeway router, enabling dual-protocol serving from the same
//!    handler logic.
//!
//! # Example
//!
//! ```ignore
//! use typeway_grpc::ApiToProto;
//!
//! // Given a typeway API type:
//! type MyAPI = (
//!     GetEndpoint<UsersPath, Vec<User>>,
//!     GetEndpoint<UserByIdPath, User>,
//!     PostEndpoint<UsersPath, CreateUser, User>,
//! );
//!
//! let proto = MyAPI::to_proto("UserService", "users.v1");
//! std::fs::write("service.proto", proto).unwrap();
//! ```
//!
//! # gRPC bridge
//!
//! ```ignore
//! use typeway_grpc::bridge::GrpcBridge;
//! use typeway_grpc::service::ApiToServiceDescriptor;
//!
//! let bridge = GrpcBridge::from_api::<MyAPI>(
//!     router_service,
//!     "UserService",
//!     "users.v1",
//! );
//! ```
//!
//! # Type mapping
//!
//! Rust primitive types map to protobuf scalar types via [`ToProtoType`].
//! User-defined struct types should implement `ToProtoType` with
//! `is_message() -> true` and provide a `message_definition()`.
//!
//! Path captures default to `string` in the generated proto. Override by
//! providing custom message definitions.

pub mod bridge;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "client")]
pub mod interceptors;
pub mod codegen;
pub mod framing;
pub mod health;
pub mod mapping;
pub mod multiplex;
pub mod proto_gen;
pub mod proto_parse;
pub mod reflection;
pub mod service;
pub mod status;
pub mod streaming;
#[cfg(feature = "test-client")]
pub mod test_client;
pub mod validate;

pub use codegen::generate_typeway_from_proto;
#[cfg(feature = "client")]
pub use client::GrpcClientError;
#[cfg(feature = "client")]
pub use interceptors::{GrpcClientConfig, GrpcRequestInterceptor};
pub use framing::{decode_grpc_frame, encode_grpc_frame, FramingError};
pub use health::{HealthService, HealthStatus};
pub use mapping::{build_message, ProtoField, ToProtoType};
pub use multiplex::{is_grpc_request, GrpcMultiplexer};
pub use proto_gen::{ApiToProto, CollectRpcs, EndpointToRpc, ProtoMessage, RpcMethod};
pub use proto_parse::{parse_proto, ParsedField, ParsedMessage, ProtoFile, ProtoRpcMethod, ProtoService};
pub use reflection::ReflectionService;
pub use service::{ApiToServiceDescriptor, GrpcMethodDescriptor, GrpcServiceDescriptor};
pub use status::{http_to_grpc_code, parse_grpc_timeout, GrpcCode, GrpcStatus, IntoGrpcStatus};
pub use streaming::{BidirectionalStream, ClientStream, ServerStream};
pub use validate::{validate_proto, ProtoValidationError};
#[cfg(feature = "test-client")]
pub use test_client::{GrpcTestClient, GrpcTestResponse};
