//! gRPC client with codec abstraction and streaming.
//!
//! [`GrpcClient`] supports both JSON and binary protobuf encoding,
//! streaming via async iteration, and request interceptors.
//!
//! # Example
//!
//! ```ignore
//! use typeway_grpc::client::GrpcClient;
//! use typeway_grpc::codec::JsonCodec;
//!
//! let client = GrpcClient::new("http://localhost:3000", "UserService", "users.v1")
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

/// A gRPC client with codec abstraction.
///
/// Supports both JSON (`application/grpc+json`) and binary protobuf
/// (`application/grpc+proto`) encoding. The codec is selected at
/// construction time.
pub struct GrpcClient {
    inner: reqwest::Client,
    base_url: url::Url,
    service_path: String,
    codec: Arc<dyn GrpcCodec>,
    config: GrpcClientConfig,
}

/// Configuration for a native gRPC client.
#[derive(Clone)]
pub struct GrpcClientConfig {
    /// Default metadata (headers) sent with every request.
    pub default_metadata: Vec<(String, String)>,
    /// Per-request timeout. `None` means no timeout.
    pub timeout: Option<Duration>,
    /// Request interceptors applied in order before sending.
    pub interceptors: Vec<GrpcRequestInterceptor>,
    /// Retry policy for transient failures. `None` means no retries.
    pub retry: Option<GrpcRetryPolicy>,
    /// Circuit breaker for fail-fast on unhealthy upstreams.
    pub circuit_breaker: Option<CircuitBreaker>,
}

/// A function that modifies outgoing gRPC requests before they are sent.
pub type GrpcRequestInterceptor =
    Arc<dyn Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder + Send + Sync>;

impl Default for GrpcClientConfig {
    fn default() -> Self {
        GrpcClientConfig {
            default_metadata: Vec::new(),
            timeout: Some(Duration::from_secs(30)),
            interceptors: Vec::new(),
            retry: None,
            circuit_breaker: None,
        }
    }
}

/// Retry policy for gRPC requests with exponential backoff and jitter.
#[derive(Debug, Clone)]
pub struct GrpcRetryPolicy {
    /// Maximum retry attempts (default: 3).
    pub max_retries: u32,
    /// Initial backoff before first retry (default: 100ms).
    pub initial_backoff: Duration,
    /// Maximum backoff cap (default: 10s).
    pub max_backoff: Duration,
    /// Backoff multiplier per attempt (default: 2.0).
    pub multiplier: f64,
    /// gRPC codes that trigger a retry.
    pub retry_on: Vec<GrpcCode>,
}

impl Default for GrpcRetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            multiplier: 2.0,
            retry_on: vec![
                GrpcCode::Unavailable,
                GrpcCode::ResourceExhausted,
                GrpcCode::DeadlineExceeded,
            ],
        }
    }
}

impl GrpcRetryPolicy {
    fn backoff_for(&self, attempt: u32) -> Duration {
        let base = self.initial_backoff.as_millis() as f64
            * self.multiplier.powi(attempt as i32);
        let capped = base.min(self.max_backoff.as_millis() as f64);
        let jitter = capped * 0.25 * pseudo_random_f64();
        Duration::from_millis((capped + jitter) as u64)
    }

    fn should_retry(&self, code: GrpcCode) -> bool {
        self.retry_on.contains(&code)
    }
}

fn pseudo_random_f64() -> f64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64);
    (hasher.finish() % 1000) as f64 / 1000.0
}

/// A circuit breaker that prevents cascading failures.
///
/// Three states: **Closed** (normal), **Open** (fail fast),
/// **HalfOpen** (probe request allowed after reset timeout).
#[derive(Clone)]
pub struct CircuitBreaker {
    state: Arc<std::sync::Mutex<CircuitBreakerState>>,
    failure_threshold: u32,
    reset_timeout: Duration,
}

#[derive(Debug)]
struct CircuitBreakerState {
    failures: u32,
    state: BreakerState,
    last_failure: Option<std::time::Instant>,
}

#[derive(Debug, PartialEq)]
enum BreakerState { Closed, Open, HalfOpen }

impl CircuitBreaker {
    /// Create a circuit breaker.
    ///
    /// - `failure_threshold`: consecutive failures before opening
    /// - `reset_timeout`: time before allowing a probe request
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        CircuitBreaker {
            state: Arc::new(std::sync::Mutex::new(CircuitBreakerState {
                failures: 0,
                state: BreakerState::Closed,
                last_failure: None,
            })),
            failure_threshold,
            reset_timeout,
        }
    }

    /// Check if a request is allowed.
    pub fn allow_request(&self) -> bool {
        let mut s = self.state.lock().unwrap();
        match s.state {
            BreakerState::Closed | BreakerState::HalfOpen => true,
            BreakerState::Open => {
                if let Some(last) = s.last_failure {
                    if last.elapsed() >= self.reset_timeout {
                        s.state = BreakerState::HalfOpen;
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Record a successful request (resets to Closed).
    pub fn record_success(&self) {
        let mut s = self.state.lock().unwrap();
        s.failures = 0;
        s.state = BreakerState::Closed;
    }

    /// Record a failed request (may transition to Open).
    pub fn record_failure(&self) {
        let mut s = self.state.lock().unwrap();
        s.failures += 1;
        s.last_failure = Some(std::time::Instant::now());
        if s.failures >= self.failure_threshold {
            s.state = BreakerState::Open;
        }
    }
}

impl std::fmt::Debug for GrpcClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrpcClientConfig")
            .field("default_metadata", &self.default_metadata)
            .field("timeout", &self.timeout)
            .field(
                "interceptors",
                &format!("[{} interceptors]", self.interceptors.len()),
            )
            .finish()
    }
}

impl GrpcClientConfig {
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
pub enum GrpcClientError {
    /// The server returned a non-OK gRPC status.
    Status {
        code: GrpcCode,
        message: String,
        /// Structured error details, if the server provided them.
        details: Vec<crate::error_details::ErrorDetail>,
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

impl GrpcClientError {
    /// Get the structured error details, if this is a `Status` error with details.
    pub fn rich_details(&self) -> &[crate::error_details::ErrorDetail] {
        match self {
            Self::Status { details, .. } => details,
            _ => &[],
        }
    }

    /// Get the gRPC status code, if this is a `Status` error.
    pub fn code(&self) -> Option<GrpcCode> {
        match self {
            Self::Status { code, .. } => Some(*code),
            _ => None,
        }
    }
}

impl std::fmt::Display for GrpcClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Status { code, message, .. } => {
                write!(f, "gRPC error {}: {}", code.as_i32(), message)
            }
            Self::Transport(e) => write!(f, "transport error: {e}"),
            Self::Codec(e) => write!(f, "codec error: {e}"),
            Self::InvalidUrl(e) => write!(f, "invalid URL: {e}"),
            Self::Framing(e) => write!(f, "framing error: {e}"),
        }
    }
}

impl std::error::Error for GrpcClientError {}

impl From<CodecError> for GrpcClientError {
    fn from(e: CodecError) -> Self {
        GrpcClientError::Codec(e)
    }
}

impl From<reqwest::Error> for GrpcClientError {
    fn from(e: reqwest::Error) -> Self {
        GrpcClientError::Transport(e.to_string())
    }
}

/// A streaming response that yields individual gRPC messages.
pub struct ClientStream {
    frames: Vec<serde_json::Value>,
    index: usize,
}

impl ClientStream {
    /// Receive the next message from the stream.
    pub async fn recv(&mut self) -> Option<Result<serde_json::Value, GrpcClientError>> {
        if self.index < self.frames.len() {
            let val = self.frames[self.index].clone();
            self.index += 1;
            Some(Ok(val))
        } else {
            None
        }
    }

    /// Collect all remaining messages into a `Vec`.
    pub async fn collect(self) -> Result<Vec<serde_json::Value>, GrpcClientError> {
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

impl GrpcClient {
    /// Create a new native gRPC client with the default JSON codec.
    pub fn new(
        base_url: &str,
        service_name: &str,
        package: &str,
    ) -> Result<Self, GrpcClientError> {
        Self::with_codec(base_url, service_name, package, Arc::new(JsonCodec))
    }

    /// Create a client with a specific codec.
    pub fn with_codec(
        base_url: &str,
        service_name: &str,
        package: &str,
        codec: Arc<dyn GrpcCodec>,
    ) -> Result<Self, GrpcClientError> {
        Self::with_codec_and_config(
            base_url,
            service_name,
            package,
            codec,
            GrpcClientConfig::default(),
        )
    }

    /// Create a client with a specific codec and configuration.
    pub fn with_codec_and_config(
        base_url: &str,
        service_name: &str,
        package: &str,
        codec: Arc<dyn GrpcCodec>,
        config: GrpcClientConfig,
    ) -> Result<Self, GrpcClientError> {
        let base_url = url::Url::parse(base_url)
            .map_err(|e| GrpcClientError::InvalidUrl(e.to_string()))?;

        let http_client = reqwest::Client::builder()
            .http2_prior_knowledge()
            .build()
            .map_err(|e| GrpcClientError::Transport(e.to_string()))?;

        let service_path = format!("{package}.{service_name}");

        Ok(GrpcClient {
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
    ) -> Result<Self, GrpcClientError> {
        let base_url = url::Url::parse(base_url)
            .map_err(|e| GrpcClientError::InvalidUrl(e.to_string()))?;
        let service_path = format!("{package}.{service_name}");

        Ok(GrpcClient {
            inner: client,
            base_url,
            service_path,
            codec: Arc::new(JsonCodec),
            config: GrpcClientConfig::default(),
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
    ) -> Result<serde_json::Value, GrpcClientError> {
        let response_bytes = self.send_request(method, request).await?;

        // Decode gRPC frame.
        let unframed = framing::decode_grpc_frame(&response_bytes)
            .map_err(|e| GrpcClientError::Framing(e.to_string()))?;

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
    ) -> Result<ClientStream, GrpcClientError> {
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
    ///
    /// Applies retry policy and circuit breaker if configured.
    async fn send_request(
        &self,
        method: &str,
        request: &serde_json::Value,
    ) -> Result<Bytes, GrpcClientError> {
        let max_attempts = self.config.retry.as_ref()
            .map(|r| r.max_retries + 1)
            .unwrap_or(1);

        let mut last_err = None;

        for attempt in 0..max_attempts {
            // Circuit breaker check.
            if let Some(ref cb) = self.config.circuit_breaker {
                if !cb.allow_request() {
                    return Err(GrpcClientError::Status {
                        code: GrpcCode::Unavailable,
                        message: "circuit breaker open".to_string(),
                        details: Vec::new(),
                    });
                }
            }

            // Backoff before retry (not before first attempt).
            if attempt > 0 {
                if let Some(ref policy) = self.config.retry {
                    tokio::time::sleep(policy.backoff_for(attempt - 1)).await;
                }
            }

            match self.send_request_once(method, request).await {
                Ok(body) => {
                    if let Some(ref cb) = self.config.circuit_breaker {
                        cb.record_success();
                    }
                    return Ok(body);
                }
                Err(e) => {
                    if let Some(ref cb) = self.config.circuit_breaker {
                        cb.record_failure();
                    }

                    // Check if we should retry.
                    let should_retry = if let Some(ref policy) = self.config.retry {
                        match &e {
                            GrpcClientError::Status { code, .. } => policy.should_retry(*code),
                            GrpcClientError::Transport(_) => true,
                            _ => false,
                        }
                    } else {
                        false
                    };

                    if should_retry && attempt + 1 < max_attempts {
                        last_err = Some(e);
                        continue;
                    }

                    return Err(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| GrpcClientError::Transport(
            "all retry attempts exhausted".to_string(),
        )))
    }

    /// Send a single gRPC request (no retry).
    async fn send_request_once(
        &self,
        method: &str,
        request: &serde_json::Value,
    ) -> Result<Bytes, GrpcClientError> {
        let grpc_path = format!("/{}/{}", self.service_path, method);
        let url = self
            .base_url
            .join(&grpc_path)
            .map_err(|e| GrpcClientError::InvalidUrl(e.to_string()))?;

        let encoded = self.codec.encode(request)?;
        let framed = framing::encode_grpc_frame(&encoded);

        let mut req = self
            .inner
            .post(url)
            .header("content-type", self.codec.content_type())
            .header("te", "trailers")
            .body(framed);

        for (key, value) in &self.config.default_metadata {
            req = req.header(key.as_str(), value.as_str());
        }
        if let Some(timeout) = self.config.timeout {
            req = req.timeout(timeout);
        }
        for interceptor in &self.config.interceptors {
            req = interceptor(req);
        }

        let response = req.send().await?;

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
            let body = response.bytes().await.unwrap_or_default();
            let details = if body.is_empty() {
                Vec::new()
            } else {
                let unframed = crate::framing::decode_grpc_frame(&body).unwrap_or_default();
                #[allow(clippy::needless_borrow)]
                crate::error_details::parse_rich_status(&unframed)
                    .or_else(|| crate::error_details::parse_rich_status(&body))
                    .map(|s| s.details)
                    .unwrap_or_default()
            };
            return Err(GrpcClientError::Status {
                code: GrpcCode::from_i32(grpc_status),
                message: grpc_message,
                details,
            });
        }

        let body = response.bytes().await?;
        Ok(body)
    }
}

/// A typed gRPC client generated by `grpc_client!`.
///
/// This macro generates a client struct with typed methods for each
/// gRPC endpoint, using the native client infrastructure.
///
/// # Example
///
/// ```ignore
/// grpc_client! {
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
macro_rules! grpc_client {
    (
        $(#[$meta:meta])*
        $vis:vis struct $Name:ident;
        api = $Api:ty;
        service = $service:expr;
        package = $package:expr;
    ) => {
        $(#[$meta])*
        $vis struct $Name {
            inner: $crate::client::GrpcClient,
        }

        impl $Name {
            /// Create a new client with the default JSON codec.
            pub fn new(base_url: &str) -> Result<Self, $crate::client::GrpcClientError> {
                Ok(Self {
                    inner: $crate::client::GrpcClient::new(
                        base_url, $service, $package,
                    )?,
                })
            }

            /// Create a client with a specific codec.
            pub fn with_codec(
                base_url: &str,
                codec: ::std::sync::Arc<dyn $crate::codec::GrpcCodec>,
            ) -> Result<Self, $crate::client::GrpcClientError> {
                Ok(Self {
                    inner: $crate::client::GrpcClient::with_codec(
                        base_url, $service, $package, codec,
                    )?,
                })
            }

            /// Create a client with a specific codec and configuration.
            pub fn with_config(
                base_url: &str,
                codec: ::std::sync::Arc<dyn $crate::codec::GrpcCodec>,
                config: $crate::client::GrpcClientConfig,
            ) -> Result<Self, $crate::client::GrpcClientError> {
                Ok(Self {
                    inner: $crate::client::GrpcClient::with_codec_and_config(
                        base_url, $service, $package, codec, config,
                    )?,
                })
            }

            /// Make a unary gRPC call by method name.
            pub async fn call(
                &self,
                method: &str,
                request: &serde_json::Value,
            ) -> Result<serde_json::Value, $crate::client::GrpcClientError> {
                self.inner.call(method, request).await
            }

            /// Make a server-streaming gRPC call by method name.
            pub async fn call_server_stream(
                &self,
                method: &str,
                request: &serde_json::Value,
            ) -> Result<$crate::client::ClientStream, $crate::client::GrpcClientError>
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

// ---------------------------------------------------------------------------
// Connection pool
// ---------------------------------------------------------------------------

/// A shared HTTP/2 connection pool for creating multiple [`GrpcClient`] instances.
///
/// All clients created from the same pool share the underlying `reqwest::Client`
/// and its HTTP/2 connection pool. This avoids creating a new TCP connection
/// per client and enables HTTP/2 multiplexing across services.
///
/// # Example
///
/// ```ignore
/// let pool = GrpcClientPool::new()
///     .pool_max_idle_per_host(10)
///     .connect_timeout(Duration::from_secs(5));
///
/// let users_client = pool.client("http://users:3000", "UserService", "users.v1")?;
/// let orders_client = pool.client("http://orders:3000", "OrderService", "orders.v1")?;
/// // Both share the same connection pool.
/// ```
pub struct GrpcClientPool {
    inner: reqwest::Client,
}

/// Builder for [`GrpcClientPool`].
pub struct GrpcClientPoolBuilder {
    pool_max_idle_per_host: usize,
    connect_timeout: Option<Duration>,
    timeout: Option<Duration>,
}

impl Default for GrpcClientPoolBuilder {
    fn default() -> Self {
        Self {
            pool_max_idle_per_host: 32,
            connect_timeout: Some(Duration::from_secs(5)),
            timeout: None,
        }
    }
}

impl GrpcClientPoolBuilder {
    /// Maximum idle connections kept alive per host (default: 32).
    pub fn pool_max_idle_per_host(mut self, max: usize) -> Self {
        self.pool_max_idle_per_host = max;
        self
    }

    /// Timeout for establishing a new connection (default: 5s).
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Overall request timeout (default: none — uses per-client config).
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Build the connection pool.
    pub fn build(self) -> Result<GrpcClientPool, GrpcClientError> {
        let mut builder = reqwest::Client::builder()
            .http2_prior_knowledge()
            .pool_max_idle_per_host(self.pool_max_idle_per_host);

        if let Some(t) = self.connect_timeout {
            builder = builder.connect_timeout(t);
        }
        if let Some(t) = self.timeout {
            builder = builder.timeout(t);
        }

        let client = builder
            .build()
            .map_err(|e| GrpcClientError::Transport(e.to_string()))?;

        Ok(GrpcClientPool { inner: client })
    }
}

impl GrpcClientPool {
    /// Create a pool builder with default settings.
    pub fn builder() -> GrpcClientPoolBuilder {
        GrpcClientPoolBuilder::default()
    }

    /// Create a [`GrpcClient`] that shares this pool's connections.
    pub fn client(
        &self,
        base_url: &str,
        service_name: &str,
        package: &str,
    ) -> Result<GrpcClient, GrpcClientError> {
        GrpcClient::with_client(base_url, service_name, package, self.inner.clone())
    }

    /// Create a [`GrpcClient`] with a specific codec that shares this pool's connections.
    pub fn client_with_codec(
        &self,
        base_url: &str,
        service_name: &str,
        package: &str,
        codec: Arc<dyn GrpcCodec>,
        config: GrpcClientConfig,
    ) -> Result<GrpcClient, GrpcClientError> {
        let base_url = url::Url::parse(base_url)
            .map_err(|e| GrpcClientError::InvalidUrl(e.to_string()))?;
        let service_path = format!("{package}.{service_name}");

        Ok(GrpcClient {
            inner: self.inner.clone(),
            base_url,
            service_path,
            codec,
            config,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_client_error_display() {
        let err = GrpcClientError::Status {
            code: GrpcCode::NotFound,
            message: "user not found".into(),
            details: Vec::new(),
        };
        assert!(err.to_string().contains("5"));
        assert!(err.to_string().contains("user not found"));

        let err = GrpcClientError::Transport("connection refused".into());
        assert!(err.to_string().contains("connection refused"));

        let err = GrpcClientError::InvalidUrl("bad url".into());
        assert!(err.to_string().contains("bad url"));
    }

    #[test]
    fn native_client_config_builder() {
        let config = GrpcClientConfig::default()
            .bearer_auth("token123")
            .metadata("x-custom", "value")
            .timeout(Duration::from_secs(5));

        assert_eq!(config.default_metadata.len(), 2);
        assert_eq!(config.timeout, Some(Duration::from_secs(5)));
    }

    #[test]
    fn native_client_config_no_timeout() {
        let config = GrpcClientConfig::default().no_timeout();
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
        let result = GrpcClient::new(
            "http://localhost:50051",
            "UserService",
            "users.v1",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn native_client_invalid_url() {
        let result = GrpcClient::new(
            "not a url",
            "Svc",
            "pkg",
        );
        assert!(result.is_err());
    }
}
