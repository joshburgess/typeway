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

    /// The request timed out.
    #[error("request timed out")]
    Timeout,

    /// All retry attempts were exhausted.
    #[error("all {attempts} retry attempts exhausted: {last_error}")]
    RetryExhausted {
        /// The error from the final attempt.
        last_error: Box<ClientError>,
        /// Total number of attempts made (initial + retries).
        attempts: u32,
    },
}

impl ClientError {
    /// Returns `true` if this error represents a timeout.
    pub fn is_timeout(&self) -> bool {
        match self {
            ClientError::Timeout => true,
            ClientError::Request(e) => e.is_timeout(),
            _ => false,
        }
    }
}
