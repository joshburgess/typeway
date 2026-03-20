//! HTTP status code to gRPC status code mapping.
//!
//! [`GrpcCode`] mirrors the standard gRPC status codes, and
//! [`http_to_grpc_code`] translates HTTP status codes to the closest
//! gRPC equivalent. This avoids requiring `tonic` as a dependency for
//! the mapping layer.

/// gRPC status codes (matching `tonic::Code` values).
///
/// Defined here to avoid requiring `tonic` as a dependency for the mapping.
/// The discriminant values match the gRPC specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum GrpcCode {
    /// The operation completed successfully.
    Ok = 0,
    /// The operation was cancelled.
    Cancelled = 1,
    /// Unknown error.
    Unknown = 2,
    /// The client specified an invalid argument.
    InvalidArgument = 3,
    /// The deadline expired before the operation could complete.
    DeadlineExceeded = 4,
    /// The requested entity was not found.
    NotFound = 5,
    /// The entity that a client attempted to create already exists.
    AlreadyExists = 6,
    /// The caller does not have permission.
    PermissionDenied = 7,
    /// Some resource has been exhausted (e.g., rate limit).
    ResourceExhausted = 8,
    /// The operation is not implemented or not supported.
    Unimplemented = 12,
    /// Internal server error.
    Internal = 13,
    /// The service is currently unavailable.
    Unavailable = 14,
    /// The caller is not authenticated.
    Unauthenticated = 16,
}

impl GrpcCode {
    /// Return the integer value of this gRPC code.
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

impl std::fmt::Display for GrpcCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            GrpcCode::Ok => "OK",
            GrpcCode::Cancelled => "CANCELLED",
            GrpcCode::Unknown => "UNKNOWN",
            GrpcCode::InvalidArgument => "INVALID_ARGUMENT",
            GrpcCode::DeadlineExceeded => "DEADLINE_EXCEEDED",
            GrpcCode::NotFound => "NOT_FOUND",
            GrpcCode::AlreadyExists => "ALREADY_EXISTS",
            GrpcCode::PermissionDenied => "PERMISSION_DENIED",
            GrpcCode::ResourceExhausted => "RESOURCE_EXHAUSTED",
            GrpcCode::Unimplemented => "UNIMPLEMENTED",
            GrpcCode::Internal => "INTERNAL",
            GrpcCode::Unavailable => "UNAVAILABLE",
            GrpcCode::Unauthenticated => "UNAUTHENTICATED",
        };
        f.write_str(name)
    }
}

/// Convert an HTTP status code to a gRPC status code.
///
/// This mapping follows the conventions described in the gRPC-over-HTTP/2
/// specification and Google's API design guide.
///
/// # Examples
///
/// ```
/// use typeway_grpc::status::{http_to_grpc_code, GrpcCode};
///
/// assert_eq!(http_to_grpc_code(http::StatusCode::OK), GrpcCode::Ok);
/// assert_eq!(http_to_grpc_code(http::StatusCode::NOT_FOUND), GrpcCode::NotFound);
/// assert_eq!(http_to_grpc_code(http::StatusCode::INTERNAL_SERVER_ERROR), GrpcCode::Internal);
/// ```
pub fn http_to_grpc_code(http_status: http::StatusCode) -> GrpcCode {
    match http_status.as_u16() {
        200..=299 => GrpcCode::Ok,
        400 => GrpcCode::InvalidArgument,
        401 => GrpcCode::Unauthenticated,
        403 => GrpcCode::PermissionDenied,
        404 => GrpcCode::NotFound,
        409 => GrpcCode::AlreadyExists,
        429 => GrpcCode::ResourceExhausted,
        499 => GrpcCode::Cancelled,
        500 => GrpcCode::Internal,
        501 => GrpcCode::Unimplemented,
        503 => GrpcCode::Unavailable,
        504 => GrpcCode::DeadlineExceeded,
        _ => GrpcCode::Unknown,
    }
}

/// Trait for converting error types to gRPC status responses.
///
/// Implement this on your error types to provide rich gRPC error information
/// beyond the default HTTP-to-gRPC status code mapping.
///
/// # Example
///
/// ```ignore
/// use typeway_grpc::status::{IntoGrpcStatus, GrpcStatus};
///
/// struct AppError { code: u16, message: String }
///
/// impl IntoGrpcStatus for AppError {
///     fn into_grpc_status(&self) -> GrpcStatus {
///         match self.code {
///             404 => GrpcStatus::not_found(&self.message),
///             401 => GrpcStatus::unauthenticated(&self.message),
///             _ => GrpcStatus::internal(&self.message),
///         }
///     }
/// }
/// ```
#[allow(clippy::wrong_self_convention)]
pub trait IntoGrpcStatus {
    /// Convert this value into a [`GrpcStatus`].
    ///
    /// Takes `&self` rather than `self` because error values are often
    /// borrowed (e.g., logged before conversion).
    fn into_grpc_status(&self) -> GrpcStatus;
}

/// A gRPC status with code and message.
#[derive(Debug, Clone)]
pub struct GrpcStatus {
    /// The gRPC status code.
    pub code: GrpcCode,
    /// A human-readable error message.
    pub message: String,
}

impl GrpcStatus {
    /// Successful status with no message.
    pub fn ok() -> Self {
        GrpcStatus {
            code: GrpcCode::Ok,
            message: String::new(),
        }
    }

    /// The requested entity was not found.
    pub fn not_found(msg: &str) -> Self {
        GrpcStatus {
            code: GrpcCode::NotFound,
            message: msg.to_string(),
        }
    }

    /// The client specified an invalid argument.
    pub fn invalid_argument(msg: &str) -> Self {
        GrpcStatus {
            code: GrpcCode::InvalidArgument,
            message: msg.to_string(),
        }
    }

    /// The caller is not authenticated.
    pub fn unauthenticated(msg: &str) -> Self {
        GrpcStatus {
            code: GrpcCode::Unauthenticated,
            message: msg.to_string(),
        }
    }

    /// The caller does not have permission.
    pub fn permission_denied(msg: &str) -> Self {
        GrpcStatus {
            code: GrpcCode::PermissionDenied,
            message: msg.to_string(),
        }
    }

    /// Internal server error.
    pub fn internal(msg: &str) -> Self {
        GrpcStatus {
            code: GrpcCode::Internal,
            message: msg.to_string(),
        }
    }

    /// The operation is not implemented or not supported.
    pub fn unimplemented(msg: &str) -> Self {
        GrpcStatus {
            code: GrpcCode::Unimplemented,
            message: msg.to_string(),
        }
    }

    /// The service is currently unavailable.
    pub fn unavailable(msg: &str) -> Self {
        GrpcStatus {
            code: GrpcCode::Unavailable,
            message: msg.to_string(),
        }
    }

    /// The entity that the client attempted to create already exists.
    pub fn already_exists(msg: &str) -> Self {
        GrpcStatus {
            code: GrpcCode::AlreadyExists,
            message: msg.to_string(),
        }
    }

    /// Some resource has been exhausted (e.g., rate limit).
    pub fn resource_exhausted(msg: &str) -> Self {
        GrpcStatus {
            code: GrpcCode::ResourceExhausted,
            message: msg.to_string(),
        }
    }

    /// Convert to gRPC response headers.
    ///
    /// Always includes `grpc-status`. Includes `grpc-message` only when
    /// the message is non-empty.
    pub fn to_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![("grpc-status".to_string(), self.code.as_i32().to_string())];
        if !self.message.is_empty() {
            headers.push(("grpc-message".to_string(), self.message.clone()));
        }
        headers
    }
}

/// Default impl for [`http::StatusCode`] — uses the existing
/// [`http_to_grpc_code`] mapping.
impl IntoGrpcStatus for http::StatusCode {
    fn into_grpc_status(&self) -> GrpcStatus {
        GrpcStatus {
            code: http_to_grpc_code(*self),
            message: self.canonical_reason().unwrap_or("").to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_codes_map_to_ok() {
        assert_eq!(http_to_grpc_code(http::StatusCode::OK), GrpcCode::Ok);
        assert_eq!(
            http_to_grpc_code(http::StatusCode::CREATED),
            GrpcCode::Ok
        );
        assert_eq!(
            http_to_grpc_code(http::StatusCode::NO_CONTENT),
            GrpcCode::Ok
        );
    }

    #[test]
    fn client_error_codes() {
        assert_eq!(
            http_to_grpc_code(http::StatusCode::BAD_REQUEST),
            GrpcCode::InvalidArgument
        );
        assert_eq!(
            http_to_grpc_code(http::StatusCode::UNAUTHORIZED),
            GrpcCode::Unauthenticated
        );
        assert_eq!(
            http_to_grpc_code(http::StatusCode::FORBIDDEN),
            GrpcCode::PermissionDenied
        );
        assert_eq!(
            http_to_grpc_code(http::StatusCode::NOT_FOUND),
            GrpcCode::NotFound
        );
        assert_eq!(
            http_to_grpc_code(http::StatusCode::CONFLICT),
            GrpcCode::AlreadyExists
        );
        assert_eq!(
            http_to_grpc_code(http::StatusCode::TOO_MANY_REQUESTS),
            GrpcCode::ResourceExhausted
        );
    }

    #[test]
    fn server_error_codes() {
        assert_eq!(
            http_to_grpc_code(http::StatusCode::INTERNAL_SERVER_ERROR),
            GrpcCode::Internal
        );
        assert_eq!(
            http_to_grpc_code(http::StatusCode::NOT_IMPLEMENTED),
            GrpcCode::Unimplemented
        );
        assert_eq!(
            http_to_grpc_code(http::StatusCode::SERVICE_UNAVAILABLE),
            GrpcCode::Unavailable
        );
        assert_eq!(
            http_to_grpc_code(http::StatusCode::GATEWAY_TIMEOUT),
            GrpcCode::DeadlineExceeded
        );
    }

    #[test]
    fn unmapped_codes_return_unknown() {
        assert_eq!(
            http_to_grpc_code(http::StatusCode::IM_A_TEAPOT),
            GrpcCode::Unknown
        );
        assert_eq!(
            http_to_grpc_code(http::StatusCode::GONE),
            GrpcCode::Unknown
        );
    }

    #[test]
    fn grpc_code_display() {
        assert_eq!(format!("{}", GrpcCode::Ok), "OK");
        assert_eq!(format!("{}", GrpcCode::NotFound), "NOT_FOUND");
        assert_eq!(format!("{}", GrpcCode::Internal), "INTERNAL");
    }

    #[test]
    fn grpc_code_as_i32() {
        assert_eq!(GrpcCode::Ok.as_i32(), 0);
        assert_eq!(GrpcCode::NotFound.as_i32(), 5);
        assert_eq!(GrpcCode::Internal.as_i32(), 13);
        assert_eq!(GrpcCode::Unauthenticated.as_i32(), 16);
    }
}
