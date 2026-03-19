//! The [`TypedResponse`] wrapper — a deserialized body paired with HTTP metadata.

use http::header::HeaderValue;
use http::{HeaderMap, StatusCode};

/// A response with both the deserialized body and HTTP metadata.
///
/// Returned by [`Client::call_full`](crate::Client::call_full) and
/// [`RequestBuilder::send_full`](crate::RequestBuilder::send_full).
///
/// # Example
///
/// ```ignore
/// let resp = client.call_full::<GetUserEndpoint>((42u32,)).await?;
/// println!("status: {}", resp.status);
/// println!("user: {:?}", resp.body);
/// if let Some(etag) = resp.header("etag") {
///     println!("etag: {etag:?}");
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TypedResponse<T> {
    /// The deserialized response body.
    pub body: T,
    /// The HTTP status code.
    pub status: StatusCode,
    /// The response headers.
    pub headers: HeaderMap,
}

impl<T> TypedResponse<T> {
    /// Get a specific header value by name.
    pub fn header(&self, name: &str) -> Option<&HeaderValue> {
        self.headers.get(name)
    }
}
