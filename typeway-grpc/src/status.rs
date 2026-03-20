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
