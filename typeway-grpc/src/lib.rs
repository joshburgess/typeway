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
pub mod mapping;
pub mod proto_gen;
pub mod service;
pub mod status;

pub use mapping::{build_message, ProtoField, ToProtoType};
pub use proto_gen::{ApiToProto, CollectRpcs, EndpointToRpc, ProtoMessage, RpcMethod};
pub use service::{ApiToServiceDescriptor, GrpcMethodDescriptor, GrpcServiceDescriptor};
pub use status::{http_to_grpc_code, GrpcCode};
