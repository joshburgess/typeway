//! Server-streaming gRPC endpoint marker.
//!
//! [`ServerStream`] wraps an endpoint type to indicate that the corresponding
//! gRPC method uses server-side streaming. The REST handler returns a normal
//! response (e.g., `Vec<T>`), and the gRPC bridge converts it to a stream of
//! individual messages.

use std::marker::PhantomData;

use typeway_core::api::ApiSpec;

/// Marks an endpoint as a server-streaming gRPC endpoint.
///
/// When used in an API type, the corresponding gRPC method uses
/// `returns (stream ResponseType)` instead of `returns (ResponseType)`.
///
/// The REST handler returns `Vec<T>` or a streaming body as normal.
/// The gRPC bridge converts this to a server-stream of individual messages.
///
/// # Example
///
/// ```ignore
/// use typeway_core::*;
/// use typeway_grpc::streaming::ServerStream;
///
/// type API = (
///     // Normal unary RPC
///     GetEndpoint<UserByIdPath, Json<User>>,
///     // Server-streaming RPC
///     ServerStream<GetEndpoint<UsersPath, Json<Vec<User>>>>,
/// );
/// ```
pub struct ServerStream<E>(PhantomData<E>);

impl<E: ApiSpec> ApiSpec for ServerStream<E> {}
