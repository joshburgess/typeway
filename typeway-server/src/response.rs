//! The [`IntoResponse`] trait and implementations.
//!
//! Any type implementing `IntoResponse` can be returned from a handler.

use bytes::Bytes;
use http::StatusCode;
use serde::Serialize;

use crate::body::{body_from_bytes, body_from_string, empty_body, BoxBody};

/// Trait for types that can be converted into an HTTP response.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as an HTTP response",
    label = "does not implement `IntoResponse`",
    note = "valid response types include: `&'static str`, `String`, `Json<T>`, `StatusCode`, `(StatusCode, T)`, `Result<T, E>`"
)]
pub trait IntoResponse {
    /// Convert this value into an HTTP response.
    fn into_response(self) -> http::Response<BoxBody>;
}

impl IntoResponse for http::Response<BoxBody> {
    fn into_response(self) -> http::Response<BoxBody> {
        self
    }
}

impl IntoResponse for &'static str {
    fn into_response(self) -> http::Response<BoxBody> {
        let body = body_from_bytes(Bytes::from_static(self.as_bytes()));
        let mut res = http::Response::new(body);
        res.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        res
    }
}

impl IntoResponse for String {
    fn into_response(self) -> http::Response<BoxBody> {
        let body = body_from_string(self);
        let mut res = http::Response::new(body);
        res.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        res
    }
}

impl IntoResponse for StatusCode {
    fn into_response(self) -> http::Response<BoxBody> {
        let mut res = http::Response::new(empty_body());
        *res.status_mut() = self;
        res
    }
}

impl<T: IntoResponse> IntoResponse for (StatusCode, T) {
    fn into_response(self) -> http::Response<BoxBody> {
        let mut res = self.1.into_response();
        *res.status_mut() = self.0;
        res
    }
}

impl<T: IntoResponse, E: IntoResponse> IntoResponse for Result<T, E> {
    fn into_response(self) -> http::Response<BoxBody> {
        match self {
            Ok(v) => v.into_response(),
            Err(e) => e.into_response(),
        }
    }
}

impl IntoResponse for Bytes {
    fn into_response(self) -> http::Response<BoxBody> {
        let body = body_from_bytes(self);
        let mut res = http::Response::new(body);
        res.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/octet-stream"),
        );
        res
    }
}

/// A JSON response wrapper.
///
/// Serializes `T` as JSON and sets `Content-Type: application/json`.
///
/// # Example
///
/// ```
/// use typeway_server::Json;
///
/// #[derive(serde::Serialize)]
/// struct User { id: u32, name: String }
///
/// async fn get_user() -> Json<User> {
///     Json(User { id: 1, name: "Alice".into() })
/// }
/// ```
pub struct Json<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> http::Response<BoxBody> {
        match serde_json::to_vec(&self.0) {
            Ok(bytes) => {
                let body = body_from_bytes(Bytes::from(bytes));
                let mut res = http::Response::new(body);
                res.headers_mut().insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/json"),
                );
                res
            }
            Err(e) => {
                let body = body_from_string(format!("JSON serialization error: {e}"));
                let mut res = http::Response::new(body);
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                res
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_str_response() {
        let res = "hello".into_response();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
    }

    #[test]
    fn string_response() {
        let res = "hello".to_string().into_response();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn status_code_response() {
        let res = StatusCode::NOT_FOUND.into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn tuple_status_body() {
        let res = (StatusCode::CREATED, "done").into_response();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    #[test]
    fn result_ok() {
        let res: Result<&str, StatusCode> = Ok("good");
        let res = res.into_response();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn result_err() {
        let res: Result<&str, StatusCode> = Err(StatusCode::BAD_REQUEST);
        let res = res.into_response();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn json_response() {
        let res = Json(serde_json::json!({"id": 1})).into_response();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json"
        );
    }
}
