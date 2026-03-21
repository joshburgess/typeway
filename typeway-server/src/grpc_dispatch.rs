//! Native gRPC server dispatch.
//!
//! Direct handler dispatch for gRPC requests. Instead of translating gRPC
//! requests through REST, the native path:
//!
//! 1. Looks up the gRPC method in a HashMap (O(1))
//! 2. Decodes the message from the gRPC frame
//! 3. Builds synthetic `Parts` so existing extractors work
//! 4. Calls the handler directly
//! 5. Returns the response with real HTTP/2 trailers
//!
//! # Architecture
//!
//! The [`GrpcRouter`] maps gRPC method paths to handlers. It is built from
//! the existing REST [`Router`] by matching each gRPC method's `rest_path`
//! and `http_method` to an already-registered handler. Since `BoxedHandler`
//! is `Arc`-wrapped, the handlers are shared (not copied) between REST and
//! gRPC dispatch.

use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body_util::BodyExt;

use typeway_grpc::codec::{GrpcCodec, JsonCodec};
use typeway_grpc::framing;
use typeway_grpc::health::HealthService;
use typeway_grpc::reflection::ReflectionService;
use typeway_grpc::service::{GrpcMethodDescriptor, GrpcServiceDescriptor};
use typeway_grpc::status::{http_to_grpc_code, GrpcCode, GrpcStatus};
use typeway_grpc::trailer_body::GrpcBody;

use crate::body::{body_from_bytes, BoxBody};
use crate::extract::PathSegments;
use crate::handler::BoxedHandler;
use crate::router::{Router, RouterService};

type StateInjector = Arc<dyn Fn(&mut http::Extensions) + Send + Sync>;

// ---------------------------------------------------------------------------
// GrpcRouter
// ---------------------------------------------------------------------------

/// A gRPC method dispatch table.
///
/// Maps gRPC method paths (e.g., `/pkg.v1.Svc/GetUser`) directly to
/// handlers via HashMap lookup. This is O(1) compared to the REST
/// router's linear scan.
pub(crate) struct GrpcRouter {
    handlers: HashMap<String, GrpcRouteEntry>,
    state_injector: Option<StateInjector>,
}

struct GrpcRouteEntry {
    handler: BoxedHandler,
    method_descriptor: GrpcMethodDescriptor,
}

impl GrpcRouter {
    /// Build a `GrpcRouter` from the REST router and a service descriptor.
    ///
    /// For each gRPC method in the descriptor, looks up the corresponding
    /// REST handler by matching `http_method` and `rest_path`. Since
    /// `BoxedHandler` is `Arc`-wrapped, the handler is shared between
    /// REST and gRPC dispatch (cheap clone).
    pub(crate) fn from_router(
        router: &Router,
        descriptor: &GrpcServiceDescriptor,
    ) -> Self {
        let mut handlers = HashMap::new();

        for method in &descriptor.methods {
            if let Some(handler) = router.find_handler_by_pattern(
                &method.http_method,
                &method.rest_path,
            ) {
                handlers.insert(
                    method.full_path.clone(),
                    GrpcRouteEntry {
                        handler,
                        method_descriptor: method.clone(),
                    },
                );
            } else {
                tracing::warn!(
                    "gRPC method {} has no matching REST handler for {} {}",
                    method.full_path,
                    method.http_method,
                    method.rest_path,
                );
            }
        }

        let state_injector = router.state_injector();

        GrpcRouter {
            handlers,
            state_injector,
        }
    }

    /// Look up a handler by gRPC method path.
    fn lookup(&self, grpc_path: &str) -> Option<&GrpcRouteEntry> {
        self.handlers.get(grpc_path)
    }
}

// ---------------------------------------------------------------------------
// Synthetic request construction
// ---------------------------------------------------------------------------

/// Build synthetic `Parts` for a binary protobuf request (no body decoding).
///
/// Used for the fast path when the handler uses `Proto<T>`. The raw binary
/// bytes are passed directly as the body — no JSON intermediate. The
/// content-type is set by the caller to `application/grpc+proto` so
/// `Proto<T>` knows to use `TypewayDecode`.
fn build_synthetic_request_raw(
    original_parts: &http::request::Parts,
    method_desc: &GrpcMethodDescriptor,
    state_injector: Option<&StateInjector>,
) -> (http::request::Parts, Bytes) {
    let mut builder = http::Request::builder()
        .method(method_desc.http_method.clone());

    builder = builder.uri(
        method_desc.rest_path.parse::<http::Uri>()
            .unwrap_or_default(),
    );

    let (synthetic_parts, _) = builder.body(()).unwrap().into_parts();
    let mut parts = synthetic_parts;

    // Copy headers from original request.
    parts.headers = original_parts.headers.clone();

    // Copy extensions from original request.
    parts.extensions = original_parts.extensions.clone();

    // Build PathSegments.
    let path_str = parts.uri.path().to_string();
    let segments: Vec<String> = path_str
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    parts.extensions.insert(PathSegments(Arc::new(segments)));

    // Inject state.
    if let Some(injector) = state_injector {
        injector(&mut parts.extensions);
    }

    (parts, Bytes::new()) // body is set by caller
}

/// Build synthetic `Parts` and body bytes from a gRPC message.
///
/// This enables existing REST extractors (`Path<T>`, `State<T>`, `Json<T>`)
/// to work without modification. The gRPC message fields are mapped to:
///
/// - **Path captures**: extracted from the message and substituted into
///   the REST path template (e.g., `/users/{}` → `/users/42`)
/// - **State**: injected via the state injector from the original Router
/// - **Body**: the full message JSON, for `Json<T>` extraction
fn build_synthetic_request(
    original_parts: &http::request::Parts,
    method_desc: &GrpcMethodDescriptor,
    message_json: &serde_json::Value,
    state_injector: Option<&StateInjector>,
) -> (http::request::Parts, Bytes) {
    // Start with a synthetic request matching the REST endpoint.
    let mut builder = http::Request::builder()
        .method(method_desc.http_method.clone());

    // Build the URI by substituting captures into the rest_path template.
    let rest_path = &method_desc.rest_path;
    let uri = if rest_path.contains("{}") {
        // Extract field values from the message to fill path captures.
        // The proto_gen module names captures param1, param2, etc., or uses
        // the field name from the first few message fields positionally.
        let capture_values = extract_capture_values(message_json, rest_path);
        let mut path = rest_path.clone();
        for val in &capture_values {
            path = path.replacen("{}", val, 1);
        }
        path
    } else {
        rest_path.clone()
    };

    builder = builder.uri(
        uri.parse::<http::Uri>()
            .unwrap_or_else(|_| method_desc.rest_path.parse().unwrap_or_default()),
    );

    // Preserve original headers (metadata, auth tokens, etc.).
    let (synthetic_parts, _) = builder.body(()).unwrap().into_parts();

    let mut parts = synthetic_parts;

    // Copy relevant headers from the original gRPC request.
    parts.headers = original_parts.headers.clone();
    // Override content-type for REST handler.
    parts
        .headers
        .insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );

    // Copy extensions from original request.
    parts.extensions = original_parts.extensions.clone();

    // Build PathSegments for the Path<T> extractor.
    let path_str = parts.uri.path().to_string();
    let segments: Vec<String> = path_str
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    parts
        .extensions
        .insert(PathSegments(Arc::new(segments)));

    // Inject state if available.
    if let Some(injector) = state_injector {
        injector(&mut parts.extensions);
    }

    // Serialize the full message as body bytes for Json<T> extraction.
    let body_bytes = serde_json::to_vec(message_json).unwrap_or_default();

    (parts, Bytes::from(body_bytes))
}

/// Extract field values from the message JSON to fill path captures.
///
/// The REST path template has `{}` placeholders. The proto generation
/// module maps path captures to message fields named `param1`, `param2`, etc.
/// We try those first, then fall back to using the first N string/number
/// fields from the message object.
fn extract_capture_values(message: &serde_json::Value, rest_path: &str) -> Vec<String> {
    let placeholder_count = rest_path.matches("{}").count();
    if placeholder_count == 0 {
        return Vec::new();
    }

    let obj = match message.as_object() {
        Some(o) => o,
        None => return vec!["".to_string(); placeholder_count],
    };

    let mut values = Vec::with_capacity(placeholder_count);

    // Try named captures: param1, param2, ...
    for i in 1..=placeholder_count {
        let key = format!("param{i}");
        if let Some(val) = obj.get(&key) {
            values.push(json_value_to_string(val));
        }
    }

    // If we found all captures via paramN, use them.
    if values.len() == placeholder_count {
        return values;
    }

    // Fallback: try common ID field names.
    values.clear();
    let id_fields = ["id", "user_id", "item_id", "name", "slug"];
    for field in &id_fields {
        if values.len() >= placeholder_count {
            break;
        }
        if let Some(val) = obj.get(*field) {
            values.push(json_value_to_string(val));
        }
    }

    // If still not enough, use the first N fields.
    if values.len() < placeholder_count {
        values.clear();
        for (_, val) in obj.iter().take(placeholder_count) {
            values.push(json_value_to_string(val));
        }
    }

    // Pad with empty strings if still not enough.
    while values.len() < placeholder_count {
        values.push(String::new());
    }

    values
}

/// Convert a JSON value to a string suitable for a path segment.
fn json_value_to_string(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => val.to_string(),
    }
}

// ---------------------------------------------------------------------------
// GrpcMultiplexer
// ---------------------------------------------------------------------------

/// Multiplexer that routes gRPC requests to native handlers and REST
/// requests to the normal `RouterService`.
///
/// gRPC requests are dispatched directly to handlers via the `GrpcRouter`
/// with real HTTP/2 trailers. Exposed as `pub` so Tower layers can be
/// applied via [`GrpcServer::layer`](crate::grpc::GrpcServer::layer).
#[derive(Clone)]
pub struct GrpcMultiplexer {
    pub(crate) rest: RouterService,
    pub(crate) grpc_router: Arc<GrpcRouter>,
    pub(crate) reflection: Arc<ReflectionService>,
    pub(crate) health: HealthService,
    pub(crate) reflection_enabled: bool,
    pub(crate) grpc_spec_json: Option<Arc<String>>,
    pub(crate) grpc_docs_html: Option<Arc<String>>,
    /// Transcoder for binary protobuf support. When set, the native dispatch
    /// auto-detects `application/grpc` (binary) vs `application/grpc+json`
    /// and transcodes accordingly.
    #[cfg(feature = "grpc-proto-binary")]
    pub(crate) transcoder: Option<Arc<typeway_grpc::transcode::ProtoTranscoder>>,
}

/// Build a gRPC JSON response with OK status (for built-in services).
fn grpc_json_response(json_body: &str) -> http::Response<BoxBody> {
    let framed = framing::encode_grpc_frame(json_body.as_bytes());
    let mut res = http::Response::new(body_from_bytes(Bytes::from(framed)));
    *res.status_mut() = http::StatusCode::OK;
    res.headers_mut().insert(
        "grpc-status",
        http::HeaderValue::from_static("0"),
    );
    res.headers_mut().insert(
        "content-type",
        http::HeaderValue::from_static("application/grpc+json"),
    );
    res
}

/// Wrap a response from a REST handler into gRPC framing with real trailers.
///
/// If the response has a [`GrpcStreamMarker`] extension (set by
/// [`GrpcStream::into_response`]), the body is already gRPC-framed
/// with trailers and is passed through as-is.
///
/// Otherwise, the body is collected, gRPC-framed, and returned with a
/// `GrpcBody` that yields the data frame followed by trailers.
/// Encode response bytes for the wire, optionally transcoding to binary protobuf.
///
/// When `use_proto_binary` is true and a transcoder is available, JSON response
/// bytes are transcoded to binary protobuf. Otherwise, JSON bytes pass through.
fn encode_response_bytes(
    json_bytes: &[u8],
    _grpc_path: &str,
    _use_proto_binary: bool,
    #[cfg(feature = "grpc-proto-binary")] transcoder: &Option<Arc<typeway_grpc::transcode::ProtoTranscoder>>,
) -> (Vec<u8>, &'static str) {
    #[cfg(feature = "grpc-proto-binary")]
    if _use_proto_binary {
        if let Some(tc) = transcoder.as_ref() {
            let json_val: serde_json::Value =
                serde_json::from_slice(json_bytes).unwrap_or_default();
            match tc.encode_response(_grpc_path, &json_val) {
                Ok(proto_bytes) => return (proto_bytes, "application/grpc+proto"),
                Err(e) => {
                    tracing::warn!("proto-binary response encode failed for {}: {}", _grpc_path, e);
                    // Fall back to JSON.
                }
            }
        }
    }

    (json_bytes.to_vec(), "application/grpc+json")
}

async fn wrap_response_as_grpc(
    rest_response: http::Response<BoxBody>,
    method_desc: &GrpcMethodDescriptor,
    grpc_path: &str,
    use_proto_binary: bool,
    #[cfg(feature = "grpc-proto-binary")] transcoder: &Option<Arc<typeway_grpc::transcode::ProtoTranscoder>>,
) -> http::Response<BoxBody> {
    let (res_parts, res_body) = rest_response.into_parts();

    // If the response is already a GrpcStream, pass it through.
    if res_parts
        .extensions
        .get::<crate::grpc_stream::GrpcStreamMarker>()
        .is_some()
    {
        let mut response = http::Response::from_parts(res_parts, res_body);
        *response.status_mut() = http::StatusCode::OK;
        response.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("application/grpc+json"),
        );
        return response;
    }

    let grpc_code = http_to_grpc_code(res_parts.status);

    let res_bytes = match res_body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => Bytes::new(),
    };

    let grpc_status = GrpcStatus {
        code: grpc_code,
        message: String::new(),
    };

    // Build gRPC-framed body, optionally transcoding to binary protobuf.
    let (framed, response_content_type) = if method_desc.server_streaming
        && grpc_code == GrpcCode::Ok
    {
        // Server-streaming: split JSON array into individual frames.
        match serde_json::from_slice::<serde_json::Value>(&res_bytes) {
            Ok(serde_json::Value::Array(items)) => {
                let mut buf = Vec::new();
                let mut ct = "application/grpc+json";
                for item in &items {
                    let item_bytes = serde_json::to_vec(item).unwrap_or_default();
                    let (encoded, content_type) = encode_response_bytes(
                        &item_bytes,
                        grpc_path,
                        use_proto_binary,
                        #[cfg(feature = "grpc-proto-binary")]
                        transcoder,
                    );
                    ct = content_type;
                    buf.extend_from_slice(&framing::encode_grpc_frame(&encoded));
                }
                (buf, ct)
            }
            _ => {
                let (encoded, ct) = encode_response_bytes(
                    &res_bytes,
                    grpc_path,
                    use_proto_binary,
                    #[cfg(feature = "grpc-proto-binary")]
                    transcoder,
                );
                (framing::encode_grpc_frame(&encoded), ct)
            }
        }
    } else {
        let (encoded, ct) = encode_response_bytes(
            &res_bytes,
            grpc_path,
            use_proto_binary,
            #[cfg(feature = "grpc-proto-binary")]
            transcoder,
        );
        (framing::encode_grpc_frame(&encoded), ct)
    };

    // Build response with GrpcBody (real HTTP/2 trailers).
    let grpc_body = GrpcBody::with_status(Bytes::from(framed), grpc_status);
    let boxed_body: BoxBody = http_body_util::BodyExt::boxed_unsync(
        http_body_util::BodyExt::map_err(grpc_body, |e| match e {}),
    );

    let mut response = http::Response::new(boxed_body);
    *response.status_mut() = http::StatusCode::OK;
    response.headers_mut().insert(
        "content-type",
        response_content_type.parse().expect("valid content-type"),
    );
    // Also set grpc-status in headers (in addition to trailers) so that
    // simple clients (reqwest, GrpcTestClient) that can't read HTTP/2
    // trailers still see the status code.
    response.headers_mut().insert(
        "grpc-status",
        grpc_code
            .as_i32()
            .to_string()
            .parse()
            .expect("valid grpc-status"),
    );
    response
}

/// Build an error response with GrpcBody trailers AND grpc-status in headers.
fn grpc_error_response(status: GrpcStatus) -> http::Response<BoxBody> {
    let code = status.code;
    let message = status.message.clone();
    let grpc_body = GrpcBody::error(status);
    let boxed_body: BoxBody = http_body_util::BodyExt::boxed_unsync(
        http_body_util::BodyExt::map_err(grpc_body, |e| match e {}),
    );
    let mut res = http::Response::new(boxed_body);
    *res.status_mut() = http::StatusCode::OK;
    res.headers_mut().insert(
        "content-type",
        http::HeaderValue::from_static("application/grpc"),
    );
    res.headers_mut().insert(
        "grpc-status",
        code.as_i32()
            .to_string()
            .parse()
            .expect("valid grpc-status"),
    );
    if !message.is_empty() {
        if let Ok(val) = message.parse() {
            res.headers_mut().insert("grpc-message", val);
        }
    }
    res
}

impl tower_service::Service<http::Request<hyper::body::Incoming>> for GrpcMultiplexer {
    type Response = http::Response<BoxBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<hyper::body::Incoming>) -> Self::Future {
        // Serve gRPC spec and docs as REST endpoints.
        let path = req.uri().path();
        if req.method() == http::Method::GET && path == "/grpc-spec" {
            if let Some(spec_json) = self.grpc_spec_json.clone() {
                return Box::pin(async move {
                    let mut res = http::Response::new(body_from_bytes(
                        Bytes::from(spec_json.as_bytes().to_vec()),
                    ));
                    res.headers_mut().insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/json; charset=utf-8"),
                    );
                    Ok(res)
                });
            }
        }
        if req.method() == http::Method::GET && path == "/grpc-docs" {
            if let Some(docs_html) = self.grpc_docs_html.clone() {
                return Box::pin(async move {
                    let mut res = http::Response::new(body_from_bytes(
                        Bytes::from(docs_html.as_bytes().to_vec()),
                    ));
                    res.headers_mut().insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("text/html; charset=utf-8"),
                    );
                    Ok(res)
                });
            }
        }

        if typeway_grpc::is_grpc_request(&req) {
            let grpc_router = self.grpc_router.clone();
            let reflection = self.reflection.clone();
            let health = self.health.clone();
            let reflection_enabled = self.reflection_enabled;
            #[cfg(feature = "grpc-proto-binary")]
            let transcoder = self.transcoder.clone();

            Box::pin(async move {
                let grpc_path = req.uri().path().to_string();

                // 1. Health check service.
                if HealthService::is_health_path(&grpc_path) {
                    let response_json = health.handle_request();
                    return Ok(grpc_json_response(&response_json));
                }

                // 2. Server reflection service.
                if reflection_enabled && ReflectionService::is_reflection_path(&grpc_path) {
                    let (parts, body) = req.into_parts();
                    let body_bytes = match body.collect().await {
                        Ok(collected) => collected.to_bytes(),
                        Err(_) => Bytes::new(),
                    };
                    let unframed = framing::decode_grpc_frame(&body_bytes)
                        .unwrap_or(&body_bytes);
                    let body_str = String::from_utf8_lossy(unframed);
                    let _ = parts;
                    let response_json = reflection.handle_request(&body_str);
                    return Ok(grpc_json_response(&response_json));
                }

                // 3. Application-defined gRPC methods — native dispatch.
                let entry = grpc_router.lookup(&grpc_path);
                let entry = match entry {
                    Some(e) => e,
                    None => {
                        let status = GrpcStatus::unimplemented(
                            &format!("method '{}' not found in service", grpc_path),
                        );
                        return Ok(grpc_error_response(status));
                    }
                };

                let method_desc = entry.method_descriptor.clone();
                let handler = entry.handler.clone();

                // Parse grpc-timeout for deadline propagation.
                let grpc_timeout = req
                    .headers()
                    .get("grpc-timeout")
                    .and_then(|v| v.to_str().ok())
                    .and_then(typeway_grpc::parse_grpc_timeout);

                // Detect whether the client sent binary protobuf or JSON.
                #[cfg(feature = "grpc-proto-binary")]
                let incoming_content_type = typeway_grpc::transcode::grpc_content_type(req.headers()).to_string();
                #[cfg(feature = "grpc-proto-binary")]
                let use_proto_binary = transcoder.is_some()
                    && typeway_grpc::transcode::is_proto_binary_content_type(&incoming_content_type);

                // Collect body and decode gRPC frame.
                let (parts, body) = req.into_parts();
                let body_bytes = match body.collect().await {
                    Ok(collected) => collected.to_bytes(),
                    Err(_) => Bytes::new(),
                };

                let unframed = framing::decode_grpc_frame(&body_bytes)
                    .map(|b| b.to_vec())
                    .unwrap_or_else(|_| body_bytes.to_vec());

                // For binary protobuf requests without path captures, pass raw
                // bytes directly to the handler. If the handler uses Proto<T>,
                // it will decode via TypewayDecode — no JSON intermediate.
                #[cfg(feature = "grpc-proto-binary")]
                let binary_fast_path = use_proto_binary
                    && !method_desc.rest_path.contains("{}");

                #[cfg(not(feature = "grpc-proto-binary"))]
                let binary_fast_path = false;

                let (synthetic_parts, body_bytes) = if binary_fast_path {
                    // Fast path: pass raw binary bytes with protobuf content-type.
                    // Proto<T> extractor detects this and uses TypewayDecode.
                    let mut synthetic = build_synthetic_request_raw(
                        &parts,
                        &method_desc,
                        grpc_router.state_injector.as_ref(),
                    );
                    synthetic.0.headers.insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/grpc+proto"),
                    );
                    (synthetic.0, Bytes::from(unframed))
                } else {
                    // JSON path: decode to serde_json::Value, serialize to body.
                    #[cfg(feature = "grpc-proto-binary")]
                    let message = if use_proto_binary {
                        let tc = transcoder.as_ref().unwrap();
                        match tc.decode_request(&grpc_path, &unframed) {
                            Ok(json) => json,
                            Err(e) => {
                                let status = GrpcStatus {
                                    code: GrpcCode::InvalidArgument,
                                    message: format!("failed to decode binary protobuf: {e}"),
                                };
                                return Ok(grpc_error_response(status));
                            }
                        }
                    } else {
                        match JsonCodec.decode(&unframed) {
                            Ok(msg) => msg,
                            Err(e) => {
                                let status = GrpcStatus {
                                    code: GrpcCode::InvalidArgument,
                                    message: format!("failed to decode request: {e}"),
                                };
                                return Ok(grpc_error_response(status));
                            }
                        }
                    };

                    #[cfg(not(feature = "grpc-proto-binary"))]
                    let message = match JsonCodec.decode(&unframed) {
                        Ok(msg) => msg,
                        Err(e) => {
                            let status = GrpcStatus {
                                code: GrpcCode::InvalidArgument,
                                message: format!("failed to decode request: {e}"),
                            };
                            return Ok(grpc_error_response(status));
                        }
                    };

                    build_synthetic_request(
                        &parts,
                        &method_desc,
                        &message,
                        grpc_router.state_injector.as_ref(),
                    )
                };

                // Call the handler, with optional timeout.
                let rest_response = if let Some(timeout_duration) = grpc_timeout {
                    match tokio::time::timeout(
                        timeout_duration,
                        handler(synthetic_parts, body_bytes),
                    )
                    .await
                    {
                        Ok(res) => res,
                        Err(_) => {
                            let status = GrpcStatus {
                                code: GrpcCode::DeadlineExceeded,
                                message: "deadline exceeded".to_string(),
                            };
                            return Ok(grpc_error_response(status));
                        }
                    }
                } else {
                    handler(synthetic_parts, body_bytes).await
                };

                // Wrap REST response as gRPC with real trailers.
                #[cfg(feature = "grpc-proto-binary")]
                let use_binary = use_proto_binary;
                #[cfg(not(feature = "grpc-proto-binary"))]
                let use_binary = false;

                Ok(wrap_response_as_grpc(
                    rest_response,
                    &method_desc,
                    &grpc_path,
                    use_binary,
                    #[cfg(feature = "grpc-proto-binary")]
                    &transcoder,
                ).await)
            })
        } else {
            // Regular REST request.
            let mut rest = self.rest.clone();
            Box::pin(async move { tower_service::Service::call(&mut rest, req).await })
        }
    }
}
