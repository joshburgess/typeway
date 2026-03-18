//! Shared body type used throughout the server.

use bytes::Bytes;
use http_body_util::Full;

/// The response body type used by wayward handlers.
pub type BoxBody = Full<Bytes>;

/// Create a `BoxBody` from bytes.
pub fn body_from_bytes(bytes: Bytes) -> BoxBody {
    Full::new(bytes)
}

/// Create a `BoxBody` from a string.
pub fn body_from_string(s: String) -> BoxBody {
    Full::new(Bytes::from(s))
}

/// Create an empty `BoxBody`.
pub fn empty_body() -> BoxBody {
    Full::new(Bytes::new())
}
