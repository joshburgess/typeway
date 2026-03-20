//! gRPC client interceptors and configuration.
//!
//! [`GrpcClientConfig`] provides a builder-style API for configuring gRPC
//! clients with default metadata, timeouts, and request interceptors.
//!
//! # Example
//!
//! ```
//! use typeway_grpc::interceptors::GrpcClientConfig;
//! use std::time::Duration;
//!
//! let config = GrpcClientConfig::default()
//!     .bearer_auth("my-token")
//!     .metadata("x-request-id", "abc123")
//!     .timeout(Duration::from_secs(10))
//!     .interceptor(|req| req.header("x-custom", "value"));
//! ```

use std::sync::Arc;
use std::time::Duration;

/// A function that modifies outgoing gRPC requests before they are sent.
///
/// Receives a [`reqwest::RequestBuilder`] and returns a modified one.
/// Interceptors are applied in the order they are added.
pub type GrpcRequestInterceptor =
    Arc<dyn Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder + Send + Sync>;

/// Configuration for a gRPC client.
///
/// Controls default metadata (headers), per-request timeouts, and request
/// interceptors. Use the builder methods to construct a configuration, then
/// pass it to the generated client's `with_config` constructor.
///
/// # Defaults
///
/// - Timeout: 30 seconds
/// - No default metadata
/// - No interceptors
#[derive(Clone)]
pub struct GrpcClientConfig {
    /// Default metadata (headers) sent with every gRPC request.
    pub default_metadata: Vec<(String, String)>,
    /// Per-request timeout. `None` means no timeout.
    pub timeout: Option<Duration>,
    /// Request interceptors applied in order before sending.
    pub interceptors: Vec<GrpcRequestInterceptor>,
}

impl Default for GrpcClientConfig {
    fn default() -> Self {
        GrpcClientConfig {
            default_metadata: Vec::new(),
            timeout: Some(Duration::from_secs(30)),
            interceptors: Vec::new(),
        }
    }
}

impl GrpcClientConfig {
    /// Add a metadata key-value pair sent with every request.
    ///
    /// Metadata is sent as HTTP headers. Keys are lowercased per HTTP/2
    /// convention. This method can be called multiple times to add multiple
    /// metadata entries.
    pub fn metadata(mut self, key: &str, value: &str) -> Self {
        self.default_metadata
            .push((key.to_string(), value.to_string()));
        self
    }

    /// Set a bearer authentication token as metadata.
    ///
    /// This is a convenience method equivalent to:
    /// ```ignore
    /// config.metadata("authorization", &format!("Bearer {}", token))
    /// ```
    pub fn bearer_auth(self, token: &str) -> Self {
        self.metadata("authorization", &format!("Bearer {}", token))
    }

    /// Set the per-request timeout.
    ///
    /// Defaults to 30 seconds. Use [`no_timeout`](Self::no_timeout) to disable.
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
    ///
    /// Interceptors are called in the order they are added. Each interceptor
    /// receives and returns a [`reqwest::RequestBuilder`], allowing it to add
    /// headers, query parameters, or other modifications.
    pub fn interceptor<F>(mut self, f: F) -> Self
    where
        F: Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder + Send + Sync + 'static,
    {
        self.interceptors.push(Arc::new(f));
        self
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
