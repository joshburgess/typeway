//! Client configuration with timeout, retry, interceptor, and header settings.

use std::fmt;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::interceptor::{RequestInterceptor, ResponseInterceptor};
use crate::retry::RetryPolicy;

/// Configuration for the [`Client`](crate::Client).
///
/// Controls per-request timeouts, TCP connect timeouts, retry behavior,
/// request/response interceptors, default headers, and cookie persistence.
///
/// # Example
///
/// ```
/// use typeway_client::{ClientConfig, RetryPolicy, RequestInterceptor};
/// use std::sync::Arc;
/// use std::time::Duration;
///
/// let config = ClientConfig::default()
///     .timeout(Duration::from_secs(60))
///     .retry_policy(RetryPolicy::none())
///     .bearer_auth("my-token")
///     .cookie_store(true)
///     .request_interceptor(Arc::new(|req| {
///         req.header("X-Request-Id", "abc123")
///     }));
/// ```
pub struct ClientConfig {
    /// Per-request timeout. `None` means no timeout.
    pub timeout: Option<Duration>,
    /// TCP connection timeout. `None` means no timeout.
    pub connect_timeout: Option<Duration>,
    /// Retry policy for failed requests.
    pub retry_policy: RetryPolicy,
    /// Interceptors applied to every outgoing request.
    pub request_interceptors: Vec<RequestInterceptor>,
    /// Interceptors called on every incoming response.
    pub response_interceptors: Vec<ResponseInterceptor>,
    /// Headers sent with every request.
    pub default_headers: HeaderMap,
    /// Whether to enable automatic cookie persistence across requests.
    pub cookie_store: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(30)),
            connect_timeout: Some(Duration::from_secs(10)),
            retry_policy: RetryPolicy::default(),
            request_interceptors: Vec::new(),
            response_interceptors: Vec::new(),
            default_headers: HeaderMap::new(),
            cookie_store: false,
        }
    }
}

impl Clone for ClientConfig {
    fn clone(&self) -> Self {
        Self {
            timeout: self.timeout,
            connect_timeout: self.connect_timeout,
            retry_policy: self.retry_policy.clone(),
            request_interceptors: self.request_interceptors.clone(),
            response_interceptors: self.response_interceptors.clone(),
            default_headers: self.default_headers.clone(),
            cookie_store: self.cookie_store,
        }
    }
}

impl fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientConfig")
            .field("timeout", &self.timeout)
            .field("connect_timeout", &self.connect_timeout)
            .field("retry_policy", &self.retry_policy)
            .field(
                "request_interceptors",
                &format!("[{} interceptor(s)]", self.request_interceptors.len()),
            )
            .field(
                "response_interceptors",
                &format!("[{} interceptor(s)]", self.response_interceptors.len()),
            )
            .field("default_headers", &self.default_headers)
            .field("cookie_store", &self.cookie_store)
            .finish()
    }
}

impl ClientConfig {
    /// Set the per-request timeout.
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }

    /// Disable the per-request timeout.
    pub fn no_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }

    /// Set the TCP connect timeout.
    pub fn connect_timeout(mut self, d: Duration) -> Self {
        self.connect_timeout = Some(d);
        self
    }

    /// Disable the TCP connect timeout.
    pub fn no_connect_timeout(mut self) -> Self {
        self.connect_timeout = None;
        self
    }

    /// Set the retry policy.
    pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Add a request interceptor that modifies outgoing requests.
    ///
    /// Interceptors are applied in the order they are added.
    ///
    /// # Example
    ///
    /// ```
    /// use typeway_client::{ClientConfig, RequestInterceptor};
    /// use std::sync::Arc;
    ///
    /// let config = ClientConfig::default()
    ///     .request_interceptor(Arc::new(|req| {
    ///         req.header("X-Trace-Id", "abc")
    ///     }));
    /// ```
    pub fn request_interceptor(mut self, interceptor: RequestInterceptor) -> Self {
        self.request_interceptors.push(interceptor);
        self
    }

    /// Add a response interceptor that inspects incoming responses.
    ///
    /// Interceptors are called in the order they are added.
    ///
    /// # Example
    ///
    /// ```
    /// use typeway_client::{ClientConfig, ResponseInterceptor};
    /// use std::sync::Arc;
    ///
    /// let config = ClientConfig::default()
    ///     .response_interceptor(Arc::new(|resp| {
    ///         eprintln!("status: {}", resp.status());
    ///     }));
    /// ```
    pub fn response_interceptor(mut self, interceptor: ResponseInterceptor) -> Self {
        self.response_interceptors.push(interceptor);
        self
    }

    /// Add a default header sent with every request.
    ///
    /// # Panics
    ///
    /// Panics if `name` or `value` cannot be parsed as valid HTTP header
    /// components.
    pub fn default_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.default_headers.insert(name, value);
        self
    }

    /// Convenience method to set a `Bearer` authentication token.
    ///
    /// This adds an `Authorization: Bearer <token>` header to every request.
    pub fn bearer_auth(self, token: &str) -> Self {
        let value = HeaderValue::from_str(&format!("Bearer {token}"))
            .expect("bearer token contains invalid header characters");
        self.default_header(http::header::AUTHORIZATION, value)
    }

    /// Enable or disable automatic cookie persistence across requests.
    ///
    /// When enabled, the underlying HTTP client stores cookies from responses
    /// and sends them back in subsequent requests, providing session-like
    /// behavior.
    pub fn cookie_store(mut self, enabled: bool) -> Self {
        self.cookie_store = enabled;
        self
    }
}
