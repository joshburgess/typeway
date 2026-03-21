//! Direct gRPC handler dispatch — bypasses HTTP extractor pipeline.
//!
//! For gRPC-only handlers that want maximum throughput, `DirectGrpcHandler`
//! calls the handler function directly with the decoded protobuf struct.
//! No synthetic HTTP Parts, no content-type detection, no BoxedHandler
//! trait object indirection.
//!
//! This is opt-in. The default `Proto<T>` extractor path serves both REST
//! and gRPC from the same handler. Use direct handlers only when you need
//! raw gRPC performance and don't need REST compatibility.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;

use typeway_grpc::framing;
use typeway_grpc::status::{GrpcCode, GrpcStatus};
use typeway_grpc::trailer_body::GrpcBody;
use typeway_protobuf::{TypewayDecode, TypewayEncode};

use crate::body::BoxBody;

/// A type-erased direct gRPC handler.
///
/// Takes raw protobuf bytes, decodes, calls the handler, encodes the
/// response. No HTTP abstraction layer.
pub(crate) type DirectHandler = Arc<
    dyn Fn(Bytes) -> Pin<Box<dyn Future<Output = Result<Bytes, GrpcStatus>> + Send>>
        + Send
        + Sync,
>;

/// Create a `DirectHandler` from an async function `async fn(Req) -> Resp`.
pub fn into_direct_handler<F, Fut, Req, Resp>(handler: F) -> DirectHandler
where
    F: Fn(Req) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Resp> + Send + 'static,
    Req: TypewayDecode + Send + 'static,
    Resp: TypewayEncode + Send + 'static,
{
    Arc::new(move |body: Bytes| {
        let h = handler.clone();
        Box::pin(async move {
            // Decode directly — no JSON, no extractors, no Parts.
            let req = Req::typeway_decode_bytes(body)
                .map_err(|e| GrpcStatus {
                    code: GrpcCode::InvalidArgument,
                    message: format!("decode error: {e}"),
                })?;

            // Call handler directly — no trait object, no BoxedHandler.
            let resp = h(req).await;

            // Encode directly — no IntoResponse, no boxing.
            Ok(Bytes::from(resp.encode_to_vec()))
        }) as Pin<Box<dyn Future<Output = Result<Bytes, GrpcStatus>> + Send>>
    })
}

/// Dispatch a direct handler: decode gRPC frame, call handler, encode response.
pub(crate) async fn dispatch_direct(
    handler: &DirectHandler,
    body_bytes: Bytes,
) -> http::Response<BoxBody> {
    // Strip gRPC framing.
    let unframed = match framing::decode_grpc_frame(&body_bytes) {
        Ok(bytes) => Bytes::copy_from_slice(bytes),
        Err(_) => body_bytes,
    };

    // Call the handler.
    match handler(unframed).await {
        Ok(response_bytes) => {
            // Frame the response.
            let framed = framing::encode_grpc_frame(&response_bytes);
            let grpc_body = GrpcBody::with_status(
                Bytes::from(framed),
                GrpcStatus { code: GrpcCode::Ok, message: String::new() },
            );
            let boxed: BoxBody = http_body_util::BodyExt::boxed_unsync(
                http_body_util::BodyExt::map_err(grpc_body, |e| match e {}),
            );
            let mut res = http::Response::new(boxed);
            *res.status_mut() = http::StatusCode::OK;
            res.headers_mut().insert(
                "content-type",
                http::HeaderValue::from_static("application/grpc+proto"),
            );
            res.headers_mut().insert(
                "grpc-status",
                http::HeaderValue::from_static("0"),
            );
            res
        }
        Err(status) => {
            let grpc_body = GrpcBody::error(status.clone());
            let boxed: BoxBody = http_body_util::BodyExt::boxed_unsync(
                http_body_util::BodyExt::map_err(grpc_body, |e| match e {}),
            );
            let mut res = http::Response::new(boxed);
            *res.status_mut() = http::StatusCode::OK;
            res.headers_mut().insert(
                "content-type",
                http::HeaderValue::from_static("application/grpc+proto"),
            );
            res.headers_mut().insert(
                "grpc-status",
                status.code.as_i32().to_string().parse().unwrap(),
            );
            res
        }
    }
}
