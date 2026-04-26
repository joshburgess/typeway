//! gRPC-Web protocol translation layer.
//!
//! gRPC-Web is a variant of gRPC designed for browser clients that cannot use
//! HTTP/2 trailers directly. It uses:
//!
//! - `content-type: application/grpc-web` (binary) or `application/grpc-web+json` (JSON)
//! - The same length-prefix framing as standard gRPC
//! - Trailers encoded in the response body as a trailing frame (flag byte `0x80`)
//!   instead of HTTP trailers, since HTTP/1.1 trailers are unreliable in browsers
//!
//! [`GrpcWebLayer`] is a Tower layer that transparently translates gRPC-Web
//! requests into standard gRPC requests for the inner service, and encodes
//! trailers in the response body on the way out.
//!
//! # Usage
//!
//! ```ignore
//! use typeway_grpc::web::GrpcWebLayer;
//!
//! Server::<API>::new(handlers)
//!     .with_grpc("Service", "pkg.v1")
//!     .layer(GrpcWebLayer::new())
//!     .serve(addr).await?;
//! ```

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{BufMut, Bytes, BytesMut};
use http_body_util::BodyExt;

/// The flag byte indicating a trailers frame in gRPC-Web.
///
/// In the gRPC length-prefix framing, the first byte is normally `0x00`
/// (uncompressed data) or `0x01` (compressed data). gRPC-Web reserves
/// `0x80` to indicate that the frame contains trailers encoded as
/// HTTP/1.1-style header lines (`key: value\r\n`).
pub const TRAILERS_FRAME_FLAG: u8 = 0x80;

/// Check whether a request has a gRPC-Web content-type header.
///
/// Matches both `application/grpc-web` (binary) and `application/grpc-web+json`.
/// Does NOT match standard `application/grpc` — use
/// [`is_grpc_request`](crate::multiplex::is_grpc_request) for that.
///
/// # Examples
///
/// ```
/// use typeway_grpc::web::is_grpc_web_request;
///
/// let req = http::Request::builder()
///     .header(http::header::CONTENT_TYPE, "application/grpc-web")
///     .body(())
///     .unwrap();
/// assert!(is_grpc_web_request(&req));
///
/// let req = http::Request::builder()
///     .header(http::header::CONTENT_TYPE, "application/grpc-web+json")
///     .body(())
///     .unwrap();
/// assert!(is_grpc_web_request(&req));
///
/// let req = http::Request::builder()
///     .header(http::header::CONTENT_TYPE, "application/grpc")
///     .body(())
///     .unwrap();
/// assert!(!is_grpc_web_request(&req));
/// ```
pub fn is_grpc_web_request<B>(req: &http::Request<B>) -> bool {
    req.headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("application/grpc-web"))
}

/// Encode trailers as a gRPC-Web trailers frame.
///
/// The trailers frame uses flag byte `0x80` followed by a 4-byte big-endian
/// length, then the trailers encoded as HTTP/1.1-style header lines
/// (`key: value\r\n`).
///
/// # Arguments
///
/// - `grpc_status`: The `grpc-status` code (e.g., `"0"` for OK).
/// - `grpc_message`: Optional `grpc-message` value.
///
/// # Examples
///
/// ```
/// use typeway_grpc::web::encode_trailers_frame;
///
/// let frame = encode_trailers_frame("0", None);
/// assert_eq!(frame[0], 0x80); // trailers flag
/// let len = u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]) as usize;
/// let trailer_text = std::str::from_utf8(&frame[5..5 + len]).unwrap();
/// assert!(trailer_text.contains("grpc-status: 0"));
/// ```
pub fn encode_trailers_frame(grpc_status: &str, grpc_message: Option<&str>) -> Bytes {
    let mut trailers = format!("grpc-status: {}\r\n", grpc_status);
    if let Some(msg) = grpc_message {
        trailers.push_str(&format!("grpc-message: {}\r\n", msg));
    }

    let trailer_bytes = trailers.as_bytes();
    let len = trailer_bytes.len() as u32;
    let mut frame = BytesMut::with_capacity(5 + trailer_bytes.len());
    frame.put_u8(TRAILERS_FRAME_FLAG);
    frame.put_u32(len);
    frame.extend_from_slice(trailer_bytes);
    frame.freeze()
}

/// A Tower layer that translates gRPC-Web requests into standard gRPC
/// requests and encodes trailers in the response body.
///
/// Non-gRPC-Web requests pass through unchanged.
///
/// # How it works
///
/// **Request path:**
/// 1. Detects `application/grpc-web` or `application/grpc-web+json` content type
/// 2. Rewrites the content type to `application/grpc` or `application/grpc+json`
/// 3. Forwards to the inner service as a standard gRPC request
///
/// **Response path:**
/// 1. Collects the response body
/// 2. Extracts `grpc-status` and `grpc-message` from response headers
/// 3. Appends a trailers frame (flag `0x80`) containing those values
/// 4. Sets the content type back to `application/grpc-web` or `application/grpc-web+json`
pub struct GrpcWebLayer;

impl GrpcWebLayer {
    /// Create a new gRPC-Web translation layer.
    pub fn new() -> Self {
        GrpcWebLayer
    }
}

impl Default for GrpcWebLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> tower_layer::Layer<S> for GrpcWebLayer {
    type Service = GrpcWebService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcWebService { inner }
    }
}

/// The Tower service produced by [`GrpcWebLayer`].
///
/// Wraps an inner gRPC service and handles gRPC-Web protocol translation.
/// Non-gRPC-Web requests are forwarded to the inner service unchanged.
#[derive(Clone)]
pub struct GrpcWebService<S> {
    inner: S,
}

impl<S> GrpcWebService<S> {
    /// Create a new gRPC-Web service wrapping the given inner service.
    pub fn new(inner: S) -> Self {
        GrpcWebService { inner }
    }
}

/// The body type returned by [`GrpcWebService`].
///
/// This is a simple wrapper around collected bytes, used to return the
/// combined data + trailers frame as a single body.
pub type GrpcWebBoxBody = http_body_util::combinators::UnsyncBoxBody<Bytes, GrpcWebBodyError>;

/// Error type for the gRPC-Web response body.
pub type GrpcWebBodyError = Box<dyn std::error::Error + Send + Sync>;

/// Create a [`GrpcWebBoxBody`] from bytes.
fn grpc_web_body_from_bytes(bytes: Bytes) -> GrpcWebBoxBody {
    http_body_util::Full::new(bytes)
        .map_err(|e| match e {})
        .boxed_unsync()
}

impl<S, ResBody> tower_service::Service<http::Request<hyper::body::Incoming>> for GrpcWebService<S>
where
    S: tower_service::Service<
            http::Request<hyper::body::Incoming>,
            Response = http::Response<ResBody>,
            Error = Infallible,
        > + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    ResBody: http_body::Body<Data = Bytes> + Send + 'static,
    ResBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = http::Response<GrpcWebBoxBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<hyper::body::Incoming>) -> Self::Future {
        if !is_grpc_web_request(&req) {
            // Not a gRPC-Web request — pass through unchanged, converting the
            // body type to GrpcWebBoxBody.
            let mut inner = self.inner.clone();
            return Box::pin(async move {
                let res = inner.call(req).await?;
                let (parts, body) = res.into_parts();
                let mapped = body
                    .map_err(|e| -> GrpcWebBodyError { e.into() })
                    .boxed_unsync();
                Ok(http::Response::from_parts(parts, mapped))
            });
        }

        let is_json = req
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.contains("grpc-web+json"));

        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Rewrite content-type from grpc-web to grpc for the inner service.
            let (mut parts, body) = req.into_parts();
            if is_json {
                parts.headers.insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/grpc+json"),
                );
            } else {
                parts.headers.insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/grpc"),
                );
            }

            let req = http::Request::from_parts(parts, body);
            let res = inner.call(req).await?;

            // Convert response: collect the body and append trailers as a body frame.
            let (mut parts, body) = res.into_parts();

            // Extract grpc-status and grpc-message from response headers.
            let grpc_status = parts
                .headers
                .remove("grpc-status")
                .and_then(|v| v.to_str().ok().map(|s| s.to_string()));
            let grpc_message = parts
                .headers
                .remove("grpc-message")
                .and_then(|v| v.to_str().ok().map(|s| s.to_string()));

            // Set the correct gRPC-Web content type on the response.
            if is_json {
                parts.headers.insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/grpc-web+json"),
                );
            } else {
                parts.headers.insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/grpc-web"),
                );
            }

            // Collect the inner response body.
            let body_bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(_) => Bytes::new(),
            };

            // Build the combined response: data frames + trailers frame.
            let trailers_frame = encode_trailers_frame(
                grpc_status.as_deref().unwrap_or("0"),
                grpc_message.as_deref(),
            );

            let mut combined = BytesMut::with_capacity(body_bytes.len() + trailers_frame.len());
            combined.extend_from_slice(&body_bytes);
            combined.extend_from_slice(&trailers_frame);

            let final_body = grpc_web_body_from_bytes(combined.freeze());
            Ok(http::Response::from_parts(parts, final_body))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_grpc_web_binary() {
        let req = http::Request::builder()
            .header(http::header::CONTENT_TYPE, "application/grpc-web")
            .body(())
            .unwrap();
        assert!(is_grpc_web_request(&req));
    }

    #[test]
    fn detects_grpc_web_json() {
        let req = http::Request::builder()
            .header(http::header::CONTENT_TYPE, "application/grpc-web+json")
            .body(())
            .unwrap();
        assert!(is_grpc_web_request(&req));
    }

    #[test]
    fn rejects_standard_grpc() {
        let req = http::Request::builder()
            .header(http::header::CONTENT_TYPE, "application/grpc")
            .body(())
            .unwrap();
        assert!(!is_grpc_web_request(&req));
    }

    #[test]
    fn rejects_grpc_json() {
        let req = http::Request::builder()
            .header(http::header::CONTENT_TYPE, "application/grpc+json")
            .body(())
            .unwrap();
        assert!(!is_grpc_web_request(&req));
    }

    #[test]
    fn rejects_plain_json() {
        let req = http::Request::builder()
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(())
            .unwrap();
        assert!(!is_grpc_web_request(&req));
    }

    #[test]
    fn rejects_no_content_type() {
        let req = http::Request::builder().body(()).unwrap();
        assert!(!is_grpc_web_request(&req));
    }

    #[test]
    fn trailers_frame_has_correct_flag() {
        let frame = encode_trailers_frame("0", None);
        assert_eq!(frame[0], TRAILERS_FRAME_FLAG);
    }

    #[test]
    fn trailers_frame_length_prefix() {
        let frame = encode_trailers_frame("0", None);
        let len = u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]) as usize;
        // The trailer text is everything after the 5-byte header.
        assert_eq!(frame.len(), 5 + len);
    }

    #[test]
    fn trailers_frame_contains_status() {
        let frame = encode_trailers_frame("0", None);
        let trailer_text = std::str::from_utf8(&frame[5..]).unwrap();
        assert!(trailer_text.contains("grpc-status: 0\r\n"));
    }

    #[test]
    fn trailers_frame_contains_message() {
        let frame = encode_trailers_frame("13", Some("internal error"));
        let trailer_text = std::str::from_utf8(&frame[5..]).unwrap();
        assert!(trailer_text.contains("grpc-status: 13\r\n"));
        assert!(trailer_text.contains("grpc-message: internal error\r\n"));
    }

    #[test]
    fn trailers_frame_omits_message_when_none() {
        let frame = encode_trailers_frame("5", None);
        let trailer_text = std::str::from_utf8(&frame[5..]).unwrap();
        assert!(trailer_text.contains("grpc-status: 5\r\n"));
        assert!(!trailer_text.contains("grpc-message"));
    }

    #[test]
    fn layer_constructs() {
        let layer = GrpcWebLayer::new();
        let default_layer = GrpcWebLayer;
        // Both should produce a GrpcWebService when applied.
        // We can only verify construction here (no inner service to wrap).
        let _ = layer;
        let _ = default_layer;
    }

    #[test]
    fn service_is_clone() {
        #[derive(Clone)]
        struct FakeService;

        let svc = GrpcWebService::new(FakeService);
        let _svc2 = svc.clone();
    }
}
