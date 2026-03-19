//! Client configuration with timeout and retry settings.

use std::time::Duration;

use crate::retry::RetryPolicy;

/// Configuration for the [`Client`](crate::Client).
///
/// Controls per-request timeouts, TCP connect timeouts, and retry behavior.
///
/// # Example
///
/// ```
/// use typeway_client::{ClientConfig, RetryPolicy};
/// use std::time::Duration;
///
/// let config = ClientConfig::default()
///     .timeout(Duration::from_secs(60))
///     .retry_policy(RetryPolicy::none());
/// ```
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Per-request timeout. `None` means no timeout.
    pub timeout: Option<Duration>,
    /// TCP connection timeout. `None` means no timeout.
    pub connect_timeout: Option<Duration>,
    /// Retry policy for failed requests.
    pub retry_policy: RetryPolicy,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(30)),
            connect_timeout: Some(Duration::from_secs(10)),
            retry_policy: RetryPolicy::default(),
        }
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
}
