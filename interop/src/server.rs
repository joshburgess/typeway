//! `grpc.testing.TestService` implementation built directly on
//! typeway-grpc's framing and trailer-body primitives.
//!
//! Each upstream RPC is dispatched on `req.uri().path()`. Unary methods
//! decode the prost request, run the handler, and emit a `GrpcBody`
//! response. Streaming methods consume frames from the request body via
//! [`GrpcFrameReader`] and emit frames through a [`GrpcStreamBody`]
//! channel, with HTTP/2 trailers carrying `grpc-status` / `grpc-message`.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Bytes, BytesMut};
use http_body::Body;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, BodyStream};
use prost::Message;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use http::HeaderMap;
use http_body::Frame;
use pin_project_lite::pin_project;

use typeway_grpc::framing::{decode_grpc_frame, encode_grpc_frame};
use typeway_grpc::status::{encode_grpc_message, GrpcCode, GrpcStatus};
use typeway_grpc::trailer_body::{GrpcBody, GrpcStreamBody};

use crate::testing::{
    Empty, Payload, PayloadType, ResponseParameters, SimpleRequest, SimpleResponse,
    StreamingInputCallRequest, StreamingInputCallResponse, StreamingOutputCallRequest,
    StreamingOutputCallResponse,
};

/// Unified response body type for the interop service.
///
/// Unary handlers return a [`GrpcBody`] (single data frame + trailers);
/// streaming handlers return a [`GrpcStreamBody`] (multiple data frames
/// + trailers). Both implement `http_body::Body<Data = Bytes, Error =
/// Infallible>`, so they're erased into a single concrete body type.
pub type RespBody = UnsyncBoxBody<Bytes, Infallible>;

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
    B: Body<Data = Bytes> + Send + Unpin + 'static,
    B::Error: std::fmt::Display + Send + 'static,
{
    type Response = http::Response<RespBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        Box::pin(async move {
            let path = req.uri().path().to_string();
            Ok(dispatch(path.as_str(), req).await)
        })
    }
}

async fn dispatch<B>(path: &str, req: http::Request<B>) -> http::Response<RespBody>
where
    B: Body<Data = Bytes> + Send + Unpin + 'static,
    B::Error: std::fmt::Display + Send + 'static,
{
    match path {
        "/grpc.testing.TestService/EmptyCall" => {
            let body = match collect_body(req).await {
                Ok(b) => b,
                Err(r) => return r,
            };
            unary::<Empty, Empty, _, _>(body, empty_call)
        }
        "/grpc.testing.TestService/UnaryCall"
        | "/grpc.testing.TestService/CacheableUnaryCall" => {
            let body = match collect_body(req).await {
                Ok(b) => b,
                Err(r) => return r,
            };
            unary::<SimpleRequest, SimpleResponse, _, _>(body, simple_unary_call)
        }
        "/grpc.testing.TestService/StreamingOutputCall" => {
            let body = match collect_body(req).await {
                Ok(b) => b,
                Err(r) => return r,
            };
            streaming_output_call(body)
        }
        "/grpc.testing.TestService/StreamingInputCall" => {
            let body = match collect_body(req).await {
                Ok(b) => b,
                Err(r) => return r,
            };
            streaming_input_call(body)
        }
        "/grpc.testing.TestService/FullDuplexCall"
        | "/grpc.testing.TestService/HalfDuplexCall" => full_duplex_call(req.into_body()),
        "/grpc.testing.TestService/UnimplementedCall"
        | "/grpc.testing.UnimplementedService/UnimplementedCall" => {
            error_response(GrpcStatus {
                code: GrpcCode::Unimplemented,
                message: String::new(),
            })
        }
        _ => error_response(GrpcStatus {
            code: GrpcCode::Unimplemented,
            message: format!("unknown method: {path}"),
        }),
    }
}

async fn collect_body<B>(req: http::Request<B>) -> Result<Bytes, http::Response<RespBody>>
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: std::fmt::Display,
{
    match req.into_body().collect().await {
        Ok(c) => Ok(c.to_bytes()),
        Err(e) => Err(error_response(GrpcStatus {
            code: GrpcCode::Internal,
            message: format!("body read failed: {e}"),
        })),
    }
}

fn unary<Req, Resp, F, Out>(framed: Bytes, handler: F) -> http::Response<RespBody>
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

fn simple_unary_call(req: SimpleRequest) -> Result<SimpleResponse, GrpcStatus> {
    if let Some(echo) = req.response_status.as_ref() {
        if echo.code != 0 {
            return Err(GrpcStatus {
                code: GrpcCode::from_i32(echo.code),
                message: echo.message.clone(),
            });
        }
    }

    let response_size = req.response_size.max(0) as usize;
    Ok(SimpleResponse {
        payload: Some(payload_of(response_size)),
        ..Default::default()
    })
}

fn streaming_output_call(framed: Bytes) -> http::Response<RespBody> {
    let payload = match decode_grpc_frame(&framed) {
        Ok(p) => p,
        Err(e) => {
            return error_response(GrpcStatus {
                code: GrpcCode::Internal,
                message: format!("framing error: {e:?}"),
            });
        }
    };
    let req = match StreamingOutputCallRequest::decode(payload) {
        Ok(r) => r,
        Err(e) => {
            return error_response(GrpcStatus {
                code: GrpcCode::InvalidArgument,
                message: format!("decode error: {e}"),
            });
        }
    };

    if let Some(echo) = req.response_status.as_ref() {
        if echo.code != 0 {
            return error_response(GrpcStatus {
                code: GrpcCode::from_i32(echo.code),
                message: echo.message.clone(),
            });
        }
    }

    let (tx, rx) = mpsc::channel::<Bytes>(8);
    tokio::spawn(async move {
        emit_response_parameters(req.response_parameters, &tx).await;
    });

    stream_response(GrpcStreamBody::new(rx))
}

fn streaming_input_call(framed: Bytes) -> http::Response<RespBody> {
    let mut total: i32 = 0;
    let mut cursor = framed.as_ref();
    while !cursor.is_empty() {
        let payload = match decode_grpc_frame(cursor) {
            Ok(p) => p,
            Err(e) => {
                return error_response(GrpcStatus {
                    code: GrpcCode::Internal,
                    message: format!("framing error: {e:?}"),
                });
            }
        };
        let consumed = 5 + payload.len();
        let msg = match StreamingInputCallRequest::decode(payload) {
            Ok(m) => m,
            Err(e) => {
                return error_response(GrpcStatus {
                    code: GrpcCode::InvalidArgument,
                    message: format!("decode error: {e}"),
                });
            }
        };
        if let Some(p) = msg.payload {
            total = total.saturating_add(p.body.len() as i32);
        }
        cursor = &cursor[consumed..];
    }

    ok_response(StreamingInputCallResponse {
        aggregated_payload_size: total,
    })
}

fn full_duplex_call<B>(body: B) -> http::Response<RespBody>
where
    B: Body<Data = Bytes> + Send + Unpin + 'static,
    B::Error: std::fmt::Display + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<StreamItem>(8);
    tokio::spawn(async move {
        let mut reader = GrpcFrameReader::new(body);
        loop {
            match reader.next_frame().await {
                Some(Ok(frame)) => {
                    let req = match StreamingOutputCallRequest::decode(frame) {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = tx
                                .send(StreamItem::Status(GrpcStatus {
                                    code: GrpcCode::InvalidArgument,
                                    message: format!("decode error: {e}"),
                                }))
                                .await;
                            return;
                        }
                    };
                    if let Some(echo) = req.response_status.as_ref() {
                        if echo.code != 0 {
                            let _ = tx
                                .send(StreamItem::Status(GrpcStatus {
                                    code: GrpcCode::from_i32(echo.code),
                                    message: echo.message.clone(),
                                }))
                                .await;
                            return;
                        }
                    }
                    if !emit_response_parameters_dynamic(req.response_parameters, &tx).await {
                        return;
                    }
                }
                Some(Err(e)) => {
                    let _ = tx
                        .send(StreamItem::Status(GrpcStatus {
                            code: GrpcCode::Internal,
                            message: e,
                        }))
                        .await;
                    return;
                }
                None => break,
            }
        }
    });

    let mut res = http::Response::new(UnsyncBoxBody::new(StreamingResponseBody::new(rx)));
    *res.status_mut() = http::StatusCode::OK;
    res.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/grpc+proto"),
    );
    res
}

async fn emit_response_parameters_dynamic(
    params: Vec<ResponseParameters>,
    tx: &mpsc::Sender<StreamItem>,
) -> bool {
    for p in params {
        let size = p.size.max(0) as usize;
        let resp = StreamingOutputCallResponse {
            payload: Some(payload_of(size)),
        };
        let mut buf = Vec::with_capacity(resp.encoded_len());
        resp.encode(&mut buf).expect("prost encode is infallible");
        let frame = Bytes::from(encode_grpc_frame(&buf));
        if tx.send(StreamItem::Data(frame)).await.is_err() {
            return false;
        }
    }
    true
}

async fn emit_response_parameters(params: Vec<ResponseParameters>, tx: &mpsc::Sender<Bytes>) {
    for p in params {
        let size = p.size.max(0) as usize;
        let resp = StreamingOutputCallResponse {
            payload: Some(payload_of(size)),
        };
        let mut buf = Vec::with_capacity(resp.encoded_len());
        resp.encode(&mut buf).expect("prost encode is infallible");
        let frame = Bytes::from(encode_grpc_frame(&buf));
        if tx.send(frame).await.is_err() {
            return;
        }
    }
}

fn payload_of(size: usize) -> Payload {
    Payload {
        r#type: PayloadType::Compressable as i32,
        body: vec![0u8; size],
    }
}

fn ok_response<T: Message>(msg: T) -> http::Response<RespBody> {
    let mut buf = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut buf).expect("prost encode is infallible");
    let framed = encode_grpc_frame(&buf);
    let mut res = http::Response::new(UnsyncBoxBody::new(GrpcBody::ok(Bytes::from(framed))));
    *res.status_mut() = http::StatusCode::OK;
    res.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/grpc+proto"),
    );
    res
}

fn error_response(status: GrpcStatus) -> http::Response<RespBody> {
    let mut res = http::Response::new(UnsyncBoxBody::new(GrpcBody::error(status)));
    *res.status_mut() = http::StatusCode::OK;
    res.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/grpc+proto"),
    );
    res
}

fn stream_response(body: GrpcStreamBody) -> http::Response<RespBody> {
    let mut res = http::Response::new(UnsyncBoxBody::new(body));
    *res.status_mut() = http::StatusCode::OK;
    res.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/grpc+proto"),
    );
    res
}

/// Reads gRPC-framed messages from an `http_body::Body`, buffering data
/// frames until a complete message is available.
///
/// gRPC frames are 5-byte length-prefixed; an HTTP/2 data frame may
/// contain any number of partial or complete gRPC frames, so the reader
/// has to demux them. End of body returns `None`.
struct GrpcFrameReader<B> {
    body: BodyStream<B>,
    buf: BytesMut,
    done: bool,
}

impl<B> GrpcFrameReader<B>
where
    B: Body<Data = Bytes> + Send + Unpin + 'static,
    B::Error: std::fmt::Display,
{
    fn new(body: B) -> Self {
        Self {
            body: BodyStream::new(body),
            buf: BytesMut::new(),
            done: false,
        }
    }

    async fn next_frame(&mut self) -> Option<Result<Bytes, String>> {
        loop {
            if let Some(frame) = self.try_take_frame() {
                return Some(frame);
            }
            if self.done {
                return None;
            }
            match self.body.next().await {
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        self.buf.extend_from_slice(&data);
                    }
                    // Trailer frames don't matter on the request side; we
                    // just keep pulling.
                }
                Some(Err(e)) => {
                    self.done = true;
                    return Some(Err(format!("body read failed: {e}")));
                }
                None => {
                    self.done = true;
                }
            }
        }
    }

    fn try_take_frame(&mut self) -> Option<Result<Bytes, String>> {
        if self.buf.len() < 5 {
            return None;
        }
        let flag = self.buf[0];
        let len = u32::from_be_bytes([self.buf[1], self.buf[2], self.buf[3], self.buf[4]]) as usize;
        if self.buf.len() < 5 + len {
            return None;
        }
        if flag != 0 {
            return Some(Err(format!("unexpected frame flag {flag:#x}")));
        }
        let _ = self.buf.split_to(5);
        let payload = self.buf.split_to(len).freeze();
        Some(Ok(payload))
    }
}

/// Item type for the bidi response channel: either a data frame or a
/// final status. The first `Status` item drains the channel and emits
/// trailers; if the channel closes without one, trailers default to OK.
enum StreamItem {
    Data(Bytes),
    Status(GrpcStatus),
}

pin_project! {
    /// A response body for bidi streaming RPCs whose final
    /// `grpc-status` is decided dynamically by the worker task.
    ///
    /// Unlike [`GrpcStreamBody`], whose final status is fixed at
    /// construction time, this body lets the worker emit a [`StreamItem`]
    /// of either `Data(Bytes)` (a gRPC-framed message) or
    /// `Status(GrpcStatus)` (the trailers). The first `Status` ends the
    /// stream; if the worker drops the sender without sending one,
    /// trailers carry `grpc-status: 0` (OK).
    struct StreamingResponseBody {
        #[pin]
        receiver: mpsc::Receiver<StreamItem>,
        done: bool,
    }
}

impl StreamingResponseBody {
    fn new(receiver: mpsc::Receiver<StreamItem>) -> Self {
        Self {
            receiver,
            done: false,
        }
    }
}

impl Body for StreamingResponseBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        let mut this = self.project();
        if *this.done {
            return Poll::Ready(None);
        }
        match this.receiver.as_mut().poll_recv(cx) {
            Poll::Ready(Some(StreamItem::Data(d))) => Poll::Ready(Some(Ok(Frame::data(d)))),
            Poll::Ready(Some(StreamItem::Status(s))) => {
                *this.done = true;
                Poll::Ready(Some(Ok(Frame::trailers(build_status_trailers(&s)))))
            }
            Poll::Ready(None) => {
                *this.done = true;
                let ok = GrpcStatus {
                    code: GrpcCode::Ok,
                    message: String::new(),
                };
                Poll::Ready(Some(Ok(Frame::trailers(build_status_trailers(&ok)))))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.done
    }
}

fn build_status_trailers(status: &GrpcStatus) -> HeaderMap {
    let mut trailers = HeaderMap::new();
    trailers.insert(
        "grpc-status",
        status
            .code
            .as_i32()
            .to_string()
            .parse()
            .expect("valid grpc-status value"),
    );
    if !status.message.is_empty() {
        let encoded = encode_grpc_message(&status.message);
        if let Ok(val) = encoded.parse() {
            trailers.insert("grpc-message", val);
        }
    }
    trailers
}
