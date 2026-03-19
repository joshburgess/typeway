//! Structured error responses and error-handling utilities.
//!
//! [`JsonError`] provides a standard JSON error format for API responses.
//! The `CatchPanic` layer catches panics in handlers and converts them
//! to 500 responses.

use http::StatusCode;
use serde::Serialize;

use crate::body::{body_from_bytes, body_from_string, BoxBody};
use crate::response::IntoResponse;

/// A structured JSON error response.
///
/// Serializes to `{"error": {"status": 400, "message": "..."}}`.
///
/// # Example
///
/// ```
/// use typeway_server::error::JsonError;
/// use typeway_server::Json;
///
/// #[derive(serde::Serialize)]
/// struct User { id: u32 }
///
/// async fn get_user() -> Result<Json<User>, JsonError> {
///     // Return a structured JSON error on failure:
///     Err(JsonError::not_found("user not found"))
/// }
/// ```
#[derive(Debug, Clone)]
pub struct JsonError {
    pub status: StatusCode,
    pub message: String,
}

#[derive(Serialize)]
struct JsonErrorBody {
    error: JsonErrorInner,
}

#[derive(Serialize)]
struct JsonErrorInner {
    status: u16,
    message: String,
}

impl JsonError {
    /// Create a new error with the given status code and message.
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        JsonError {
            status,
            message: message.into(),
        }
    }

    /// 400 Bad Request.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    /// 401 Unauthorized.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    /// 403 Forbidden.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    /// 404 Not Found.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    /// 409 Conflict.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }

    /// 422 Unprocessable Entity.
    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, message)
    }

    /// 500 Internal Server Error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl IntoResponse for JsonError {
    fn into_response(self) -> http::Response<BoxBody> {
        let body = JsonErrorBody {
            error: JsonErrorInner {
                status: self.status.as_u16(),
                message: self.message,
            },
        };
        match serde_json::to_vec(&body) {
            Ok(bytes) => {
                let body = body_from_bytes(bytes::Bytes::from(bytes));
                let mut res = http::Response::new(body);
                *res.status_mut() = self.status;
                res.headers_mut().insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/json"),
                );
                res
            }
            Err(e) => {
                let mut res = http::Response::new(body_from_string(format!(
                    "error serialization failed: {e}"
                )));
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                res
            }
        }
    }
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.status.as_u16(),
            self.status,
            self.message
        )
    }
}

impl std::error::Error for JsonError {}

/// Implement `From<(StatusCode, String)>` so existing extractor errors
/// can be converted to `JsonError` automatically.
impl From<(StatusCode, String)> for JsonError {
    fn from((status, message): (StatusCode, String)) -> Self {
        JsonError { status, message }
    }
}

// ---------------------------------------------------------------------------
// OpenAPI error responses (feature = "openapi")
// ---------------------------------------------------------------------------

#[cfg(feature = "openapi")]
impl typeway_openapi::ErrorResponses for JsonError {
    fn error_responses() -> indexmap::IndexMap<String, typeway_openapi::spec::Response> {
        use typeway_openapi::spec::*;

        let mut content = indexmap::IndexMap::new();
        let mut properties = indexmap::IndexMap::new();

        let mut error_props = indexmap::IndexMap::new();
        error_props.insert("status".to_string(), Schema::integer());
        error_props.insert("message".to_string(), Schema::string());

        properties.insert(
            "error".to_string(),
            Schema {
                schema_type: Some("object".into()),
                format: None,
                items: None,
                properties: Some(error_props),
                description: None,
            },
        );

        content.insert(
            "application/json".to_string(),
            MediaType {
                schema: Some(Schema {
                    schema_type: Some("object".into()),
                    format: None,
                    items: None,
                    properties: Some(properties),
                    description: Some("JSON error response".into()),
                }),
            },
        );

        let mut responses = indexmap::IndexMap::new();
        responses.insert(
            "4XX".to_string(),
            Response {
                description: "Client error".to_string(),
                content: content.clone(),
            },
        );
        responses.insert(
            "5XX".to_string(),
            Response {
                description: "Server error".to_string(),
                content,
            },
        );
        responses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_error_response() {
        let err = JsonError::not_found("user not found");
        let res = err.into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json"
        );
    }

    #[test]
    fn json_error_from_tuple() {
        let err: JsonError = (StatusCode::BAD_REQUEST, "bad input".to_string()).into();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(err.message, "bad input");
    }
}
