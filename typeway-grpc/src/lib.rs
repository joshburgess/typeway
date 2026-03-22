//! `typeway-grpc` — gRPC / Protocol Buffers interop for the Typeway web framework.
//!
//! This crate provides:
//!
//! 1. **Proto generation** — given an API type (a tuple of endpoints),
//!    [`ApiToProto::to_proto`] produces a complete `.proto` file with service
//!    and message definitions.
//!
//! 2. **gRPC dispatch** — direct handler dispatch with real HTTP/2 trailers,
//!    real streaming via `tokio::sync::mpsc`, and codec abstraction
//!    ([`JsonCodec`], [`BinaryCodec`], [`TypewayCodecAdapter`]).
//!
//! 3. **TypewayCodec** — compile-time specialized protobuf encode/decode via
//!    `#[derive(TypewayCodec)]`, 3-8x faster than runtime codecs.
//!
//! 4. **gRPC client** — [`GrpcClient`] with codec selection and streaming.
//!
//! ## Encoding
//!
//! Two encoding modes are supported:
//!
//! - **JSON mode** (default) — `application/grpc+json`. Handlers use JSON
//!   via REST extractors. The [`grpc_client!`] macro generates clients
//!   that use JSON encoding for typeway-to-typeway communication.
//!
//! - **Binary protobuf mode** (`proto-binary` feature) — `application/grpc`.
//!   Standard gRPC clients (grpcurl, tonic, Postman) interop without JSON mode.
//!   Enable with `.with_proto_binary()` on the server builder.
//!
//! # Example
//!
//! ```ignore
//! use typeway_grpc::ApiToProto;
//!
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

#[cfg(feature = "prost-build")]
pub mod build;
#[doc(hidden)]
pub mod codec;
pub mod codegen;
#[cfg(feature = "compression")]
pub mod compression;
pub mod diff;
pub mod docs_page;
pub mod error_details;
pub mod framing;
pub mod health;
pub mod mapping;
pub mod multiplex;
#[cfg(feature = "grpc-native")]
pub mod client;
#[cfg(feature = "grpc-native")]
pub mod native_streaming;
#[doc(hidden)]
pub mod proto_codec;
pub mod proto_gen;
pub mod proto_parse;
pub mod ready;
pub mod reflection;
pub mod service;
pub mod spec;
pub mod status;
pub mod streaming;
#[cfg(feature = "test-client")]
pub mod test_client;
#[cfg(feature = "tonic-compat")]
pub mod tonic_compat;
#[cfg(feature = "grpc-native")]
pub mod trailer_body;
#[cfg(feature = "proto-binary")]
pub mod transcode;
#[doc(hidden)]
pub mod typeway_codec_adapter;
pub mod validate;
pub mod web;

// --- Re-exports ---

pub use codec::{CodecError as GrpcCodecError, CodecErrorKind, GrpcCodec, JsonCodec};
#[cfg(feature = "proto-binary")]
pub use codec::{BinaryCodec, CodecDirection};
pub use codegen::{
    generate_typeway_from_proto, generate_typeway_from_proto_with_codec, proto_to_typeway,
    proto_to_typeway_with_codec,
};
#[cfg(feature = "compression")]
pub use compression::{
    compress, decode_frame_with_decompression, decompress, encode_compressed_frame,
    incoming_compression, negotiate_compression, Compression, CompressionError,
};
pub use diff::{diff_protos, ChangeKind, ProtoChange};
pub use docs_page::generate_docs_html;
pub use error_details::{
    BadRequest, DebugInfo, ErrorDetail, ErrorInfo, FieldViolation, Help, HelpLink,
    IntoRichGrpcStatus, LocalizedMessage, PreconditionFailure, PreconditionViolation, QuotaFailure,
    QuotaViolation, ResourceInfo, RetryInfo, RichGrpcStatus,
};
pub use framing::{decode_grpc_frame, decode_grpc_frames, encode_grpc_frame, FramingError};
pub use health::{HealthService, HealthStatus};
pub use mapping::{build_message, ProtoField, ToProtoType};
pub use multiplex::{is_grpc_request, GrpcMultiplexer};
#[cfg(feature = "grpc-native")]
pub use client::{
    ClientStream as GrpcClientStream, GrpcClient, GrpcClientConfig, GrpcClientError,
    GrpcRequestInterceptor,
};
#[cfg(feature = "grpc-native")]
pub use native_streaming::{
    grpc_bidi_channel, grpc_channel, grpc_channel_default, GrpcBiStream, GrpcReceiver, GrpcSender,
    StreamSendError, DEFAULT_STREAM_BUFFER,
};
pub use proto_codec::{
    decode_varint, encode_varint, json_to_proto_binary, proto_binary_to_json, wire_type_for,
    CodecError, ProtoFieldDef,
};
pub use proto_gen::{ApiToProto, CollectRpcs, EndpointToRpc, ProtoMessage, RpcMethod};
pub use proto_parse::{
    parse_proto, ParsedField, ParsedMessage, ProtoFile, ProtoRpcMethod, ProtoService,
};
pub use ready::GrpcReady;
pub use reflection::ReflectionService;
pub use service::{ApiToServiceDescriptor, GrpcMethodDescriptor, GrpcServiceDescriptor};
pub use spec::{ApiToGrpcSpec, GrpcServiceSpec};
pub use status::{http_to_grpc_code, parse_grpc_timeout, GrpcCode, GrpcStatus, IntoGrpcStatus};
pub use streaming::{BidirectionalStream, ClientStream, ServerStream};
#[cfg(feature = "test-client")]
pub use test_client::{GrpcStreamingResponse, GrpcTestClient, GrpcTestResponse};
#[cfg(feature = "tonic-compat")]
pub use tonic_compat::{json_to_prost, prost_to_json, Protobuf, ProtobufError};
#[cfg(feature = "grpc-native")]
pub use trailer_body::{GrpcBody, GrpcStreamBody};
#[cfg(feature = "proto-binary")]
pub use transcode::{
    grpc_content_type, is_grpc_json_content_type, is_proto_binary_content_type, ProtoTranscoder,
    TranscodeError,
};
pub use typeway_protobuf::{
    tw_decode_varint, tw_encode_tag, tw_encode_varint, tw_skip_wire_value, tw_tag_len,
    tw_varint_len, tw_zigzag_decode, tw_zigzag_encode, TypewayDecode, TypewayDecodeError,
    TypewayEncode,
};
pub use typeway_codec_adapter::TypewayCodecAdapter;
pub use validate::{validate_proto, ProtoValidationError};
pub use web::{
    encode_trailers_frame, is_grpc_web_request, GrpcWebLayer, GrpcWebService,
    TRAILERS_FRAME_FLAG,
};
