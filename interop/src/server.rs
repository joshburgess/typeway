//! `grpc.testing.TestService` implementation built directly on
//! typeway-grpc's framing and trailer-body primitives.
//!
//! Each upstream RPC is dispatched on `req.uri().path()`; unary RPCs decode
//! the prost request, run the handler, and emit a `GrpcBody` response with
//! proper HTTP/2 trailers. Streaming RPCs return `UNIMPLEMENTED` for now.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body_util::BodyExt;
use prost::Message;

use typeway_grpc::framing::{decode_grpc_frame, encode_grpc_frame};
use typeway_grpc::status::{GrpcCode, GrpcStatus};
use typeway_grpc::trailer_body::GrpcBody;

use crate::testing::{
    Empty, Payload, PayloadType, SimpleRequest, SimpleResponse,
};

/// Tower service for `grpc.testing.TestService`.
#[derive(Clone, Default)]
pub struct TestService;

impl TestService {
    pub fn new() -> Self {
        Self
    }
}

impl<B> tower_service::Service<http::Request<B>> for TestService
where
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: std::fmt::Display + Send + 'static,
{
    type Response = http::Response<GrpcBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        Box::pin(async move {
            let path = req.uri().path().to_string();
            let body = match req.into_body().collect().await {
                Ok(c) => c.to_bytes(),
                Err(e) => {
                    return Ok(error_response(GrpcStatus {
                        code: GrpcCode::Internal,
                        message: format!("body read failed: {e}"),
                    }));
                }
            };
            Ok(dispatch(&path, body).await)
        })
    }
}

async fn dispatch(path: &str, framed: Bytes) -> http::Response<GrpcBody> {
    match path {
        "/grpc.testing.TestService/EmptyCall" => unary::<Empty, Empty, _, _>(framed, empty_call),
        "/grpc.testing.TestService/UnaryCall" => {
            unary::<SimpleRequest, SimpleResponse, _, _>(framed, unary_call)
        }
        "/grpc.testing.TestService/CacheableUnaryCall" => {
            unary::<SimpleRequest, SimpleResponse, _, _>(framed, unary_call)
        }
        "/grpc.testing.TestService/UnimplementedCall"
        | "/grpc.testing.UnimplementedService/UnimplementedCall" => {
            error_response(GrpcStatus {
                code: GrpcCode::Unimplemented,
                message: String::new(),
            })
        }
        // Streaming methods are not yet implemented; report cleanly.
        "/grpc.testing.TestService/StreamingOutputCall"
        | "/grpc.testing.TestService/StreamingInputCall"
        | "/grpc.testing.TestService/FullDuplexCall"
        | "/grpc.testing.TestService/HalfDuplexCall" => error_response(GrpcStatus {
            code: GrpcCode::Unimplemented,
            message: "streaming interop methods not yet implemented".into(),
        }),
        _ => error_response(GrpcStatus {
            code: GrpcCode::Unimplemented,
            message: format!("unknown method: {path}"),
        }),
    }
}

fn unary<Req, Resp, F, Out>(framed: Bytes, handler: F) -> http::Response<GrpcBody>
where
    Req: Message + Default,
    Resp: Message,
    F: FnOnce(Req) -> Out,
    Out: HandlerOutcome<Resp>,
{
    let payload = match decode_grpc_frame(&framed) {
        Ok(p) => p,
        Err(e) => {
            return error_response(GrpcStatus {
                code: GrpcCode::Internal,
                message: format!("framing error: {e:?}"),
            });
        }
    };
    let req = match Req::decode(payload) {
        Ok(r) => r,
        Err(e) => {
            return error_response(GrpcStatus {
                code: GrpcCode::InvalidArgument,
                message: format!("decode error: {e}"),
            });
        }
    };
    match handler(req).into_outcome() {
        Ok(resp) => ok_response(resp),
        Err(status) => error_response(status),
    }
}

trait HandlerOutcome<T> {
    fn into_outcome(self) -> Result<T, GrpcStatus>;
}
impl<T> HandlerOutcome<T> for T
where
    T: Message,
{
    fn into_outcome(self) -> Result<T, GrpcStatus> {
        Ok(self)
    }
}
impl<T> HandlerOutcome<T> for Result<T, GrpcStatus>
where
    T: Message,
{
    fn into_outcome(self) -> Result<T, GrpcStatus> {
        self
    }
}

fn empty_call(_req: Empty) -> Empty {
    Empty {}
}

fn unary_call(req: SimpleRequest) -> Result<SimpleResponse, GrpcStatus> {
    if let Some(echo) = req.response_status.as_ref() {
        if echo.code != 0 {
            return Err(GrpcStatus {
                code: GrpcCode::from_i32(echo.code),
                message: echo.message.clone(),
            });
        }
    }

    let response_size = req.response_size.max(0) as usize;
    let payload = Payload {
        r#type: PayloadType::Compressable as i32,
        body: vec![0u8; response_size],
    };
    Ok(SimpleResponse {
        payload: Some(payload),
        ..Default::default()
    })
}

fn ok_response<T: Message>(msg: T) -> http::Response<GrpcBody> {
    let mut buf = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut buf).expect("prost encode is infallible");
    let framed = encode_grpc_frame(&buf);
    let mut res = http::Response::new(GrpcBody::ok(Bytes::from(framed)));
    *res.status_mut() = http::StatusCode::OK;
    res.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/grpc+proto"),
    );
    res
}

fn error_response(status: GrpcStatus) -> http::Response<GrpcBody> {
    let mut res = http::Response::new(GrpcBody::error(status));
    *res.status_mut() = http::StatusCode::OK;
    res.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/grpc+proto"),
    );
    res
}
