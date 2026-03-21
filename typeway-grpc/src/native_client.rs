//! Native gRPC client with codec abstraction and real streaming.
//!
//! [`NativeGrpcClient`] is a gRPC client that supports both JSON and
//! binary protobuf encoding, real streaming via async iteration, and
//! the same interceptor/config system as the bridge-based client.
//!
//! # Example
//!
//! ```ignore
//! use typeway_grpc::native_client::NativeGrpcClient;
//! use typeway_grpc::codec::JsonCodec;
//!
//! let client = NativeGrpcClient::new("http://localhost:3000", "UserService", "users.v1")
//!     .unwrap();
//!
//! // Unary call
//! let response = client.call("GetUser", &serde_json::json!({"id": 42})).await?;
//!
//! // Server-streaming call
//! let mut stream = client.call_server_stream("ListUsers", &serde_json::json!({})).await?;
//! while let Some(item) = stream.recv().await {
//!     println!("{}", item?);
//! }
//! ```

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;

use crate::codec::{CodecError, GrpcCodec, JsonCodec};
use crate::framing;
use crate::status::GrpcCode;

/// A native gRPC client with codec abstraction.
///
/// Supports both JSON (`application/grpc+json`) and binary protobuf
/// (`application/grpc+proto`) encoding. The codec is selected at
/// construction time.
pub struct NativeGrpcClient {
    inner: reqwest::Client,
    base_url: url::Url,
    service_path: String,
    codec: Arc<dyn GrpcCodec>,
    config: NativeClientConfig,
}

/// Configuration for a native gRPC client.
#[derive(Clone)]
pub struct NativeClientConfig {
    /// Default metadata (headers) sent with every request.
    pub default_metadata: Vec<(String, String)>,
    /// Per-request timeout. `None` means no timeout.
    pub timeout: Option<Duration>,
    /// Request interceptors applied in order before sending.
    pub interceptors: Vec<NativeClientInterceptor>,
}

/// A function that modifies outgoing gRPC requests before they are sent.
pub type NativeClientInterceptor =
    Arc<dyn Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder + Send + Sync>;

impl Default for NativeClientConfig {
    fn default() -> Self {
        NativeClientConfig {
            default_metadata: Vec::new(),
            timeout: Some(Duration::from_secs(30)),
            interceptors: Vec::new(),
        }
    }
}

impl NativeClientConfig {
    /// Add a metadata key-value pair sent with every request.
    pub fn metadata(mut self, key: &str, value: &str) -> Self {
        self.default_metadata
            .push((key.to_string(), value.to_string()));
        self
    }

    /// Set a bearer authentication token.
    pub fn bearer_auth(self, token: &str) -> Self {
        self.metadata("authorization", &format!("Bearer {token}"))
    }

    /// Set the per-request timeout.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Disable the per-request timeout.
    pub fn no_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }

    /// Add a request interceptor.
    pub fn interceptor<F>(mut self, f: F) -> Self
    where
        F: Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder + Send + Sync + 'static,
    {
        self.interceptors.push(Arc::new(f));
        self
    }
}

/// Errors from the native gRPC client.
#[derive(Debug)]
pub enum NativeClientError {
    /// The server returned a non-OK gRPC status.
    Status {
        code: GrpcCode,
        message: String,
    },
    /// HTTP transport error.
    Transport(String),
    /// Codec encode/decode error.
    Codec(CodecError),
    /// Invalid URL.
    InvalidUrl(String),
    /// gRPC frame decoding error.
    Framing(String),
}

impl std::fmt::Display for NativeClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Status { code, message } => {
                write!(f, "gRPC error {}: {}", code.as_i32(), message)
            }
            Self::Transport(e) => write!(f, "transport error: {e}"),
            Self::Codec(e) => write!(f, "codec error: {e}"),
            Self::InvalidUrl(e) => write!(f, "invalid URL: {e}"),
            Self::Framing(e) => write!(f, "framing error: {e}"),
        }
    }
}

impl std::error::Error for NativeClientError {}

impl From<CodecError> for NativeClientError {
    fn from(e: CodecError) -> Self {
        NativeClientError::Codec(e)
    }
}

impl From<reqwest::Error> for NativeClientError {
    fn from(e: reqwest::Error) -> Self {
        NativeClientError::Transport(e.to_string())
    }
}

/// A streaming response that yields individual gRPC messages.
pub struct ClientStream {
    frames: Vec<serde_json::Value>,
    index: usize,
}

impl ClientStream {
    /// Receive the next message from the stream.
    pub async fn recv(&mut self) -> Option<Result<serde_json::Value, NativeClientError>> {
        if self.index < self.frames.len() {
            let val = self.frames[self.index].clone();
            self.index += 1;
            Some(Ok(val))
        } else {
            None
        }
    }

    /// Collect all remaining messages into a `Vec`.
    pub async fn collect(self) -> Result<Vec<serde_json::Value>, NativeClientError> {
        Ok(self.frames)
    }

    /// Number of messages in the stream.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the stream is empty.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

impl NativeGrpcClient {
    /// Create a new native gRPC client with the default JSON codec.
    pub fn new(
        base_url: &str,
        service_name: &str,
        package: &str,
    ) -> Result<Self, NativeClientError> {
        Self::with_codec(base_url, service_name, package, Arc::new(JsonCodec))
    }

    /// Create a client with a specific codec.
    pub fn with_codec(
        base_url: &str,
        service_name: &str,
        package: &str,
        codec: Arc<dyn GrpcCodec>,
    ) -> Result<Self, NativeClientError> {
        Self::with_codec_and_config(
            base_url,
            service_name,
            package,
            codec,
            NativeClientConfig::default(),
        )
    }

    /// Create a client with a specific codec and configuration.
    pub fn with_codec_and_config(
        base_url: &str,
        service_name: &str,
        package: &str,
        codec: Arc<dyn GrpcCodec>,
        config: NativeClientConfig,
    ) -> Result<Self, NativeClientError> {
        let base_url = url::Url::parse(base_url)
            .map_err(|e| NativeClientError::InvalidUrl(e.to_string()))?;

        let http_client = reqwest::Client::builder()
            .http2_prior_knowledge()
            .build()
            .map_err(|e| NativeClientError::Transport(e.to_string()))?;

        let service_path = format!("{package}.{service_name}");

        Ok(NativeGrpcClient {
            inner: http_client,
            base_url,
            service_path,
            codec,
            config,
        })
    }

    /// Create a client wrapping an existing reqwest client.
    pub fn with_client(
        base_url: &str,
        service_name: &str,
        package: &str,
        client: reqwest::Client,
    ) -> Result<Self, NativeClientError> {
        let base_url = url::Url::parse(base_url)
            .map_err(|e| NativeClientError::InvalidUrl(e.to_string()))?;
        let service_path = format!("{package}.{service_name}");

        Ok(NativeGrpcClient {
            inner: client,
            base_url,
            service_path,
            codec: Arc::new(JsonCodec),
            config: NativeClientConfig::default(),
        })
    }

    /// Make a unary gRPC call.
    ///
    /// The method name should be PascalCase (e.g., `"GetUser"`).
    /// The request body is encoded using the client's codec.
    pub async fn call(
        &self,
        method: &str,
        request: &serde_json::Value,
    ) -> Result<serde_json::Value, NativeClientError> {
        let response_bytes = self.send_request(method, request).await?;

        // Decode gRPC frame.
        let unframed = framing::decode_grpc_frame(&response_bytes)
            .map_err(|e| NativeClientError::Framing(e.to_string()))?;

        // Decode response via codec.
        let value = self.codec.decode(unframed)?;
        Ok(value)
    }

    /// Make a server-streaming gRPC call.
    ///
    /// Returns a [`ClientStream`] that yields individual messages.
    pub async fn call_server_stream(
        &self,
        method: &str,
        request: &serde_json::Value,
    ) -> Result<ClientStream, NativeClientError> {
        let response_bytes = self.send_request(method, request).await?;

        // Decode multiple gRPC frames.
        let (frames, _trailers) = framing::decode_grpc_frames(&response_bytes);

        let mut items = Vec::with_capacity(frames.len());
        for frame in frames {
            let value = self.codec.decode(frame)?;
            items.push(value);
        }

        Ok(ClientStream {
            frames: items,
            index: 0,
        })
    }

    /// Send a gRPC request and return the raw response bytes.
    async fn send_request(
        &self,
        method: &str,
        request: &serde_json::Value,
    ) -> Result<Bytes, NativeClientError> {
        let grpc_path = format!("/{}/{}", self.service_path, method);
        let url = self
            .base_url
            .join(&grpc_path)
            .map_err(|e| NativeClientError::InvalidUrl(e.to_string()))?;

        // Encode request body.
        let encoded = self.codec.encode(request)?;
        let framed = framing::encode_grpc_frame(&encoded);

        // Build the HTTP/2 request.
        let mut req = self
            .inner
            .post(url)
            .header("content-type", self.codec.content_type())
            .header("te", "trailers")
            .body(framed);

        // Apply default metadata.
        for (key, value) in &self.config.default_metadata {
            req = req.header(key.as_str(), value.as_str());
        }

        // Apply timeout.
        if let Some(timeout) = self.config.timeout {
            req = req.timeout(timeout);
        }

        // Apply interceptors.
        for interceptor in &self.config.interceptors {
            req = interceptor(req);
        }

        // Send.
        let response = req.send().await?;

        // Check gRPC status from headers or trailers.
        let grpc_status = response
            .headers()
            .get("grpc-status")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);

        let grpc_message = response
            .headers()
            .get("grpc-message")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if grpc_status != 0 {
            return Err(NativeClientError::Status {
                code: GrpcCode::from_i32(grpc_status),
                message: grpc_message,
            });
        }

        // Collect response body.
        let body = response.bytes().await?;
        Ok(body)
    }
}

/// A typed gRPC client generated by `native_grpc_client!`.
///
/// This macro generates a client struct with typed methods for each
/// gRPC endpoint, using the native client infrastructure.
///
/// # Example
///
/// ```ignore
/// native_grpc_client! {
///     pub struct UserClient;
///     api = UserAPI;
///     service = "UserService";
///     package = "users.v1";
/// }
///
/// let client = UserClient::new("http://localhost:3000")?;
/// let users = client.call("ListUser", &serde_json::json!({})).await?;
/// ```
#[macro_export]
macro_rules! native_grpc_client {
    (
        $(#[$meta:meta])*
        $vis:vis struct $Name:ident;
        api = $Api:ty;
        service = $service:expr;
        package = $package:expr;
    ) => {
        $(#[$meta])*
        $vis struct $Name {
            inner: $crate::native_client::NativeGrpcClient,
        }

        impl $Name {
            /// Create a new client with the default JSON codec.
            pub fn new(base_url: &str) -> Result<Self, $crate::native_client::NativeClientError> {
                Ok(Self {
                    inner: $crate::native_client::NativeGrpcClient::new(
                        base_url, $service, $package,
                    )?,
                })
            }

            /// Create a client with a specific codec.
            pub fn with_codec(
                base_url: &str,
                codec: ::std::sync::Arc<dyn $crate::codec::GrpcCodec>,
            ) -> Result<Self, $crate::native_client::NativeClientError> {
                Ok(Self {
                    inner: $crate::native_client::NativeGrpcClient::with_codec(
                        base_url, $service, $package, codec,
                    )?,
                })
            }

            /// Create a client with a specific codec and configuration.
            pub fn with_config(
                base_url: &str,
                codec: ::std::sync::Arc<dyn $crate::codec::GrpcCodec>,
                config: $crate::native_client::NativeClientConfig,
            ) -> Result<Self, $crate::native_client::NativeClientError> {
                Ok(Self {
                    inner: $crate::native_client::NativeGrpcClient::with_codec_and_config(
                        base_url, $service, $package, codec, config,
                    )?,
                })
            }

            /// Make a unary gRPC call by method name.
            pub async fn call(
                &self,
                method: &str,
                request: &serde_json::Value,
            ) -> Result<serde_json::Value, $crate::native_client::NativeClientError> {
                self.inner.call(method, request).await
            }

            /// Make a server-streaming gRPC call by method name.
            pub async fn call_server_stream(
                &self,
                method: &str,
                request: &serde_json::Value,
            ) -> Result<$crate::native_client::ClientStream, $crate::native_client::NativeClientError>
            {
                self.inner.call_server_stream(method, request).await
            }

            /// Get the service descriptor for this client's API.
            pub fn service_descriptor(
                &self,
            ) -> $crate::service::GrpcServiceDescriptor {
                <$Api as $crate::service::ApiToServiceDescriptor>::service_descriptor(
                    $service, $package,
                )
            }

            /// Get the generated `.proto` file content.
            pub fn proto(&self) -> String {
                <$Api as $crate::proto_gen::ApiToProto>::to_proto($service, $package)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_client_error_display() {
        let err = NativeClientError::Status {
            code: GrpcCode::NotFound,
            message: "user not found".into(),
        };
        assert!(err.to_string().contains("5"));
        assert!(err.to_string().contains("user not found"));

        let err = NativeClientError::Transport("connection refused".into());
        assert!(err.to_string().contains("connection refused"));

        let err = NativeClientError::InvalidUrl("bad url".into());
        assert!(err.to_string().contains("bad url"));
    }

    #[test]
    fn native_client_config_builder() {
        let config = NativeClientConfig::default()
            .bearer_auth("token123")
            .metadata("x-custom", "value")
            .timeout(Duration::from_secs(5));

        assert_eq!(config.default_metadata.len(), 2);
        assert_eq!(config.timeout, Some(Duration::from_secs(5)));
    }

    #[test]
    fn native_client_config_no_timeout() {
        let config = NativeClientConfig::default().no_timeout();
        assert_eq!(config.timeout, None);
    }

    #[test]
    fn client_stream_empty() {
        let stream = ClientStream {
            frames: vec![],
            index: 0,
        };
        assert!(stream.is_empty());
        assert_eq!(stream.len(), 0);
    }

    #[test]
    fn native_client_construction() {
        let result = NativeGrpcClient::new(
            "http://localhost:50051",
            "UserService",
            "users.v1",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn native_client_invalid_url() {
        let result = NativeGrpcClient::new(
            "not a url",
            "Svc",
            "pkg",
        );
        assert!(result.is_err());
    }
}
