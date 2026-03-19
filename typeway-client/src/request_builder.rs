//! Per-call [`RequestBuilder`] for overriding headers, query params, and timeouts.
//!
//! Obtained via [`Client::request`](crate::Client::request). This allows
//! per-call customization without changing the client-wide configuration.
//!
//! # Example
//!
//! ```ignore
//! let user = client
//!     .request::<GetUserEndpoint>((42u32,))
//!     .header("X-Request-Id", "abc123")
//!     .timeout(Duration::from_secs(5))
//!     .send()
//!     .await?;
//! ```

use std::marker::PhantomData;
use std::time::Duration;

use http::header::{HeaderMap, HeaderName, HeaderValue};

use crate::call::CallEndpoint;
use crate::client::Client;
use crate::error::ClientError;
use crate::typed_response::TypedResponse;

/// A builder for a single request with per-call overrides.
///
/// Created by [`Client::request`](crate::Client::request). The builder allows
/// adding extra headers, query parameters, and a per-request timeout before
/// sending. Retries are **not** applied on the builder path — use
/// [`Client::call`](crate::Client::call) for automatic retries.
pub struct RequestBuilder<'a, E: CallEndpoint> {
    client: &'a Client,
    args: E::Args,
    extra_headers: HeaderMap,
    query_params: Vec<(String, String)>,
    timeout: Option<Duration>,
    _endpoint: PhantomData<E>,
}

impl<'a, E: CallEndpoint> RequestBuilder<'a, E> {
    pub(crate) fn new(client: &'a Client, args: E::Args) -> Self {
        Self {
            client,
            args,
            extra_headers: HeaderMap::new(),
            query_params: Vec::new(),
            timeout: None,
            _endpoint: PhantomData,
        }
    }

    /// Add a header to this specific request.
    pub fn header(mut self, name: impl Into<HeaderName>, value: impl Into<HeaderValue>) -> Self {
        self.extra_headers.insert(name.into(), value.into());
        self
    }

    /// Add a query parameter to the URL.
    pub fn query(mut self, key: &str, value: &str) -> Self {
        self.query_params.push((key.to_string(), value.to_string()));
        self
    }

    /// Override the timeout for this specific request.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Send the request and deserialize the response body.
    ///
    /// Retries are **not** applied — this executes a single attempt.
    pub async fn send(self) -> Result<E::Response, ClientError> {
        let overrides = CallOverrides {
            extra_headers: if self.extra_headers.is_empty() {
                None
            } else {
                Some(self.extra_headers)
            },
            query_string: None,
            query_params: if self.query_params.is_empty() {
                None
            } else {
                Some(self.query_params)
            },
            timeout: self.timeout,
        };
        let (_, body) = self
            .client
            .call_inner::<E>(&self.args, Some(&overrides))
            .await?;
        Ok(body)
    }

    /// Send the request and return the full response metadata alongside the body.
    ///
    /// Retries are **not** applied — this executes a single attempt.
    pub async fn send_full(self) -> Result<TypedResponse<E::Response>, ClientError> {
        let overrides = CallOverrides {
            extra_headers: if self.extra_headers.is_empty() {
                None
            } else {
                Some(self.extra_headers)
            },
            query_string: None,
            query_params: if self.query_params.is_empty() {
                None
            } else {
                Some(self.query_params)
            },
            timeout: self.timeout,
        };
        self.client
            .call_inner::<E>(&self.args, Some(&overrides))
            .await
            .map(|(meta, body)| TypedResponse {
                body,
                status: meta.status,
                headers: meta.headers,
            })
    }
}

/// Per-call overrides applied by `RequestBuilder` and `call_inner`.
pub(crate) struct CallOverrides {
    pub extra_headers: Option<HeaderMap>,
    /// Pre-encoded query string (from `serde_urlencoded`), appended as-is.
    pub query_string: Option<String>,
    /// Individual key-value query parameters.
    pub query_params: Option<Vec<(String, String)>>,
    pub timeout: Option<Duration>,
}

/// Response metadata returned alongside the deserialized body from `call_inner`.
pub(crate) struct ResponseMeta {
    pub status: http::StatusCode,
    pub headers: HeaderMap,
}
