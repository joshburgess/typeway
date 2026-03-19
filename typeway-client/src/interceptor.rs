//! Request and response interceptors for the [`Client`](crate::Client).
//!
//! Interceptors allow modifying outgoing requests and inspecting incoming
//! responses without changing individual call sites.
//!
//! # Example
//!
//! ```
//! use typeway_client::{ClientConfig, RequestInterceptor, ResponseInterceptor};
//! use std::sync::Arc;
//!
//! let config = ClientConfig::default()
//!     .request_interceptor(Arc::new(|req| {
//!         req.header("X-Custom", "value")
//!     }))
//!     .response_interceptor(Arc::new(|resp| {
//!         println!("Response status: {}", resp.status());
//!     }));
//! ```

use std::sync::Arc;

/// A function that modifies outgoing requests before they are sent.
///
/// Interceptors are applied in the order they are added. Each interceptor
/// receives and returns a [`reqwest::RequestBuilder`], allowing it to add
/// headers, query parameters, or other modifications.
pub type RequestInterceptor =
    Arc<dyn Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder + Send + Sync>;

/// A function that inspects responses after they are received.
///
/// Response interceptors cannot modify the response; they are intended for
/// logging, metrics, or side-effect-based observation. They are called in
/// the order they are added.
pub type ResponseInterceptor = Arc<dyn Fn(&reqwest::Response) + Send + Sync>;
