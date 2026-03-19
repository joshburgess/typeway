//! Built-in tracing support for the [`Client`](crate::Client).
//!
//! Enable request/response tracing by calling
//! [`ClientConfig::enable_tracing`](crate::ClientConfig::enable_tracing) or
//! by using the [`with_tracing`] convenience function.
//!
//! When tracing is enabled, every request logs the HTTP method, URL, response
//! status, and elapsed duration at `DEBUG` level via the [`tracing`] crate.
//!
//! # Example
//!
//! ```
//! use typeway_client::{ClientConfig, with_tracing};
//!
//! // Option 1: builder method
//! let config = ClientConfig::default().enable_tracing();
//!
//! // Option 2: convenience function
//! let config = with_tracing(ClientConfig::default());
//! ```

use crate::config::ClientConfig;

/// Enable built-in tracing on the given [`ClientConfig`].
///
/// This is a convenience wrapper around
/// [`ClientConfig::enable_tracing`](ClientConfig::enable_tracing) for use in
/// a functional pipeline.
///
/// # Example
///
/// ```
/// use typeway_client::{ClientConfig, RetryPolicy, with_tracing};
///
/// let config = with_tracing(
///     ClientConfig::default()
///         .retry_policy(RetryPolicy::none()),
/// );
/// ```
pub fn with_tracing(config: ClientConfig) -> ClientConfig {
    config.enable_tracing()
}
