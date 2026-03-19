//! Client error types.

use http::StatusCode;

/// Errors that can occur when making client requests.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The server returned a non-2xx status code.
    #[error("request failed with status {status}: {body}")]
    Status { status: StatusCode, body: String },

    /// Failed to build the request URL.
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),

    /// HTTP transport error.
    #[error("request error: {0}")]
    Request(#[from] reqwest::Error),

    /// Failed to deserialize the response body.
    #[error("deserialization error: {0}")]
    Deserialize(String),

    /// Failed to serialize the request body.
    #[error("serialization error: {0}")]
    Serialize(String),
}
