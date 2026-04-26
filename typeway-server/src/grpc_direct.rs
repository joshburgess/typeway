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

use typeway_grpc::status::{GrpcCode, GrpcStatus};
use typeway_grpc::trailer_body::GrpcBody;
use typeway_protobuf::{TypewayDecode, TypewayEncode};

use crate::body::BoxBody;

/// A type-erased direct gRPC handler.
pub type DirectHandler = Arc<
    dyn Fn(Bytes) -> Pin<Box<dyn Future<Output = Result<Bytes, GrpcStatus>> + Send>> + Send + Sync,
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
            let req = Req::typeway_decode_bytes(body).map_err(|e| GrpcStatus {
                code: GrpcCode::InvalidArgument,
                message: format!("decode error: {e}"),
            })?;
            let resp = h(req).await;
            Ok(Bytes::from(resp.encode_to_vec()))
        }) as Pin<Box<dyn Future<Output = Result<Bytes, GrpcStatus>> + Send>>
    })
}

/// Pre-built OK status header value (avoids per-request allocation).
static GRPC_STATUS_OK: http::HeaderValue = http::HeaderValue::from_static("0");
static CONTENT_TYPE_PROTO: http::HeaderValue =
    http::HeaderValue::from_static("application/grpc+proto");

/// Dispatch a direct handler with minimal allocation.
pub(crate) async fn dispatch_direct(
    handler: &DirectHandler,
    body_bytes: Bytes,
) -> http::Response<BoxBody> {
    // Strip gRPC framing: zero-copy via Bytes::slice.
    let unframed = if body_bytes.len() >= 5 {
        body_bytes.slice(5..)
    } else {
        body_bytes
    };

    match handler(unframed).await {
        Ok(response_bytes) => {
            // Build gRPC frame inline: 5-byte header + payload in one allocation.
            let payload_len = response_bytes.len();
            let mut framed = Vec::with_capacity(5 + payload_len);
            framed.push(0); // not compressed
            framed.extend_from_slice(&(payload_len as u32).to_be_bytes());
            framed.extend_from_slice(&response_bytes);

            let grpc_body = GrpcBody::ok(Bytes::from(framed));
            let boxed: BoxBody = http_body_util::BodyExt::boxed_unsync(
                http_body_util::BodyExt::map_err(grpc_body, |e| match e {}),
            );
            let mut res = http::Response::new(boxed);
            *res.status_mut() = http::StatusCode::OK;
            res.headers_mut()
                .insert("content-type", CONTENT_TYPE_PROTO.clone());
            res.headers_mut()
                .insert("grpc-status", GRPC_STATUS_OK.clone());
            res
        }
        Err(status) => {
            let code = status.code;
            let grpc_body = GrpcBody::error(status);
            let boxed: BoxBody = http_body_util::BodyExt::boxed_unsync(
                http_body_util::BodyExt::map_err(grpc_body, |e| match e {}),
            );
            let mut res = http::Response::new(boxed);
            *res.status_mut() = http::StatusCode::OK;
            res.headers_mut()
                .insert("content-type", CONTENT_TYPE_PROTO.clone());
            res.headers_mut()
                .insert("grpc-status", code.as_i32().to_string().parse().unwrap());
            res
        }
    }
}
