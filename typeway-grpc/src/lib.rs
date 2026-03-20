//! `typeway-grpc` — gRPC / Protocol Buffers interop for the Typeway web framework.
//!
//! This crate generates `.proto` file definitions from typeway API types.
//! Given an API type (a tuple of endpoints), [`ApiToProto::to_proto`] produces
//! a complete Protocol Buffers service definition with message types.
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
//! # Type mapping
//!
//! Rust primitive types map to protobuf scalar types via [`ToProtoType`].
//! User-defined struct types should implement `ToProtoType` with
//! `is_message() -> true` and provide a `message_definition()`.
//!
//! Path captures default to `string` in the generated proto. Override by
//! providing custom message definitions.

pub mod mapping;
pub mod proto_gen;

pub use mapping::{build_message, ProtoField, ToProtoType};
pub use proto_gen::{ApiToProto, CollectRpcs, EndpointToRpc, ProtoMessage, RpcMethod};
