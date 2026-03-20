//! Streaming gRPC endpoint markers.
//!
//! These marker types wrap endpoint types to indicate streaming behavior
//! in the generated gRPC service:
//!
//! - [`ServerStream`] — server-side streaming (`returns (stream ResponseType)`)
//! - [`ClientStream`] — client-side streaming (`rpc Method(stream RequestType)`)
//! - [`BidirectionalStream`] — bidirectional streaming (`rpc Method(stream Req) returns (stream Res)`)

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

/// Marks an endpoint as a client-streaming gRPC endpoint.
///
/// The client sends a stream of request messages; the server responds once.
/// Proto output: `rpc Method(stream RequestType) returns (ResponseType);`
///
/// # Example
///
/// ```ignore
/// use typeway_core::*;
/// use typeway_grpc::streaming::ClientStream;
///
/// type API = (
///     // Client-streaming RPC
///     ClientStream<PostEndpoint<UploadPath, Chunk, Summary>>,
/// );
/// ```
pub struct ClientStream<E>(PhantomData<E>);

impl<E: ApiSpec> ApiSpec for ClientStream<E> {}

/// Marks an endpoint as a bidirectional-streaming gRPC endpoint.
///
/// Both client and server send streams of messages.
/// Proto output: `rpc Method(stream RequestType) returns (stream ResponseType);`
///
/// # Example
///
/// ```ignore
/// use typeway_core::*;
/// use typeway_grpc::streaming::BidirectionalStream;
///
/// type API = (
///     // Bidirectional-streaming RPC
///     BidirectionalStream<GetEndpoint<ChatPath, Message>>,
/// );
/// ```
pub struct BidirectionalStream<E>(PhantomData<E>);

impl<E: ApiSpec> ApiSpec for BidirectionalStream<E> {}
