//! Security headers middleware — adds standard security headers to every response.
//!
//! Applies a configurable set of HTTP security headers. The defaults follow
//! OWASP recommendations and can be overridden or disabled individually.
//!
//! # Example
//!
//! ```ignore
//! use typeway_server::secure_headers::SecureHeadersLayer;
//!
//! Server::<API>::new(handlers)
//!     .layer(SecureHeadersLayer::new())
//!     .serve(addr)
//!     .await?;
//! ```
//!
//! # Customization
//!
//! ```ignore
//! SecureHeadersLayer::new()
//!     .hsts(63_072_000)                          // enable HSTS (TLS only)
//!     .frame_options("SAMEORIGIN")               // allow same-origin framing
//!     .content_security_policy("default-src 'self'; script-src 'self' cdn.example.com")
//!     .custom("X-Custom-Header", "value")
//! ```

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::body::BoxBody;

/// A Tower layer that adds security headers to every response.
///
/// Created via [`SecureHeadersLayer::new()`], which sets sensible defaults.
/// Individual headers can be overridden, disabled, or extended with the
/// builder methods.
#[derive(Clone, Debug)]
pub struct SecureHeadersLayer {
    headers: Vec<(String, String)>,
}

impl SecureHeadersLayer {
    /// Create a new layer with all default security headers enabled.
    ///
    /// Default headers:
    /// - `X-Content-Type-Options: nosniff`
    /// - `X-Frame-Options: DENY`
    /// - `X-XSS-Protection: 0`
    /// - `Referrer-Policy: strict-origin-when-cross-origin`
    /// - `Content-Security-Policy: default-src 'self'`
    /// - `Permissions-Policy: camera=(), microphone=(), geolocation=()`
    pub fn new() -> Self {
        SecureHeadersLayer {
            headers: vec![
                ("x-content-type-options".to_string(), "nosniff".to_string()),
                ("x-frame-options".to_string(), "DENY".to_string()),
                ("x-xss-protection".to_string(), "0".to_string()),
                (
                    "referrer-policy".to_string(),
                    "strict-origin-when-cross-origin".to_string(),
                ),
                (
                    "content-security-policy".to_string(),
                    "default-src 'self'".to_string(),
                ),
                (
                    "permissions-policy".to_string(),
                    "camera=(), microphone=(), geolocation=()".to_string(),
                ),
            ],
        }
    }

    /// Enable HTTP Strict Transport Security (HSTS) with the given max-age in seconds.
    ///
    /// This header should only be enabled when the server is behind TLS.
    /// The `includeSubDomains` and `preload` directives are included automatically.
    ///
    /// # Example
    ///
    /// ```ignore
    /// SecureHeadersLayer::new().hsts(63_072_000) // 2 years
    /// ```
    pub fn hsts(mut self, max_age_secs: u64) -> Self {
        let value = format!("max-age={max_age_secs}; includeSubDomains; preload");
        self.set_header("strict-transport-security", value);
        self
    }

    /// Override the `X-Frame-Options` header value.
    ///
    /// Common values: `"DENY"`, `"SAMEORIGIN"`.
    pub fn frame_options(mut self, value: impl Into<String>) -> Self {
        self.set_header("x-frame-options", value.into());
        self
    }

    /// Override the `Content-Security-Policy` header value.
    pub fn content_security_policy(mut self, value: impl Into<String>) -> Self {
        self.set_header("content-security-policy", value.into());
        self
    }

    /// Remove the `Content-Security-Policy` header entirely.
    pub fn disable_csp(mut self) -> Self {
        self.headers
            .retain(|(name, _)| name != "content-security-policy");
        self
    }

    /// Add an arbitrary header name/value pair.
    ///
    /// If the header already exists in the set, its value is replaced.
    pub fn custom(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let name = name.into().to_ascii_lowercase();
        let value = value.into();
        self.set_header(&name, value);
        self
    }

    /// Internal helper: set or replace a header by lowercase name.
    fn set_header(&mut self, name: &str, value: String) {
        if let Some(entry) = self.headers.iter_mut().find(|(n, _)| n == name) {
            entry.1 = value;
        } else {
            self.headers.push((name.to_string(), value));
        }
    }
}

impl Default for SecureHeadersLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> tower_layer::Layer<S> for SecureHeadersLayer {
    type Service = SecureHeadersService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        // Pre-parse header name/value pairs into http types for fast insertion.
        let parsed: Vec<(http::HeaderName, http::HeaderValue)> = self
            .headers
            .iter()
            .filter_map(|(name, value)| {
                let header_name = http::HeaderName::from_bytes(name.as_bytes()).ok()?;
                let header_value = http::HeaderValue::from_str(value).ok()?;
                Some((header_name, header_value))
            })
            .collect();

        SecureHeadersService {
            inner,
            headers: std::sync::Arc::new(parsed),
        }
    }
}

/// The Tower service produced by [`SecureHeadersLayer`].
///
/// Wraps an inner service and appends security headers to every response.
#[derive(Clone)]
pub struct SecureHeadersService<S> {
    inner: S,
    headers: std::sync::Arc<Vec<(http::HeaderName, http::HeaderValue)>>,
}

impl<S, B> tower_service::Service<http::Request<B>> for SecureHeadersService<S>
where
    S: tower_service::Service<
            http::Request<B>,
            Response = http::Response<BoxBody>,
            Error = Infallible,
        > + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = http::Response<BoxBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let mut inner = self.inner.clone();
        let headers = self.headers.clone();
        Box::pin(async move {
            let mut resp = inner.call(req).await?;
            for (name, value) in headers.iter() {
                resp.headers_mut().insert(name.clone(), value.clone());
            }
            Ok(resp)
        })
    }
}
