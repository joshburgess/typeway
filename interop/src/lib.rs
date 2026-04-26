//! gRPC interoperability test suite for typeway-grpc.
//!
//! Provides a [`TestService`](server::TestService) implementation of the
//! upstream `grpc.testing.TestService` along with a binary entry point
//! that runs it on a TCP listener. The service is the same one the
//! official gRPC interop test suite drives.

pub mod server;

/// Prost-generated message types from `proto/messages.proto`,
/// `proto/empty.proto`, and `proto/test.proto`.
pub mod testing {
    include!(concat!(env!("OUT_DIR"), "/grpc.testing.rs"));
}
