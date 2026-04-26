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
    /// The system is not in a state required for the operation.
    FailedPrecondition = 9,
    /// The operation was aborted (often due to a concurrency conflict).
    Aborted = 10,
    /// The operation was attempted past the valid range.
    OutOfRange = 11,
    /// The operation is not implemented or not supported.
    Unimplemented = 12,
    /// Internal server error.
    Internal = 13,
    /// The service is currently unavailable.
    Unavailable = 14,
    /// Unrecoverable data loss or corruption.
    DataLoss = 15,
    /// The caller is not authenticated.
    Unauthenticated = 16,
}

impl GrpcCode {
    /// Return the integer value of this gRPC code.
    pub fn as_i32(self) -> i32 {
        self as i32
    }

    /// Convert an integer to a gRPC code.
    ///
    /// Unknown values map to [`GrpcCode::Unknown`].
    ///
    /// # Example
    ///
    /// ```
    /// use typeway_grpc::status::GrpcCode;
    ///
    /// assert_eq!(GrpcCode::from_i32(0), GrpcCode::Ok);
    /// assert_eq!(GrpcCode::from_i32(5), GrpcCode::NotFound);
    /// assert_eq!(GrpcCode::from_i32(999), GrpcCode::Unknown);
    /// ```
    pub fn from_i32(code: i32) -> Self {
        match code {
            0 => GrpcCode::Ok,
            1 => GrpcCode::Cancelled,
            2 => GrpcCode::Unknown,
            3 => GrpcCode::InvalidArgument,
            4 => GrpcCode::DeadlineExceeded,
            5 => GrpcCode::NotFound,
            6 => GrpcCode::AlreadyExists,
            7 => GrpcCode::PermissionDenied,
            8 => GrpcCode::ResourceExhausted,
            9 => GrpcCode::FailedPrecondition,
            10 => GrpcCode::Aborted,
            11 => GrpcCode::OutOfRange,
            12 => GrpcCode::Unimplemented,
            13 => GrpcCode::Internal,
            14 => GrpcCode::Unavailable,
            15 => GrpcCode::DataLoss,
            16 => GrpcCode::Unauthenticated,
            _ => GrpcCode::Unknown,
        }
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
            GrpcCode::FailedPrecondition => "FAILED_PRECONDITION",
            GrpcCode::Aborted => "ABORTED",
            GrpcCode::OutOfRange => "OUT_OF_RANGE",
            GrpcCode::Unimplemented => "UNIMPLEMENTED",
            GrpcCode::Internal => "INTERNAL",
            GrpcCode::Unavailable => "UNAVAILABLE",
            GrpcCode::DataLoss => "DATA_LOSS",
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

    /// Convert to a [`RichGrpcStatus`](crate::error_details::RichGrpcStatus)
    /// with no details attached.
    ///
    /// Use this as a starting point to add structured error details:
    ///
    /// ```
    /// use typeway_grpc::status::GrpcStatus;
    /// use typeway_grpc::error_details::{BadRequest, FieldViolation};
    ///
    /// let rich = GrpcStatus::invalid_argument("bad input")
    ///     .into_rich()
    ///     .with_bad_request(BadRequest {
    ///         field_violations: vec![FieldViolation {
    ///             field: "email".to_string(),
    ///             description: "invalid format".to_string(),
    ///         }],
    ///     });
    /// ```
    pub fn into_rich(self) -> crate::error_details::RichGrpcStatus {
        crate::error_details::RichGrpcStatus {
            code: self.code.as_i32(),
            message: self.message,
            details: Vec::new(),
        }
    }

    /// Convert to gRPC response headers.
    ///
    /// Always includes `grpc-status`. Includes `grpc-message` only when
    /// the message is non-empty. The message is percent-encoded per the
    /// gRPC spec so it can survive transport in an HTTP header value.
    pub fn to_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![("grpc-status".to_string(), self.code.as_i32().to_string())];
        if !self.message.is_empty() {
            headers.push((
                "grpc-message".to_string(),
                encode_grpc_message(&self.message),
            ));
        }
        headers
    }
}

/// Percent-encode a `grpc-message` value for HTTP transport.
///
/// Per the gRPC HTTP/2 spec, `grpc-message` is conceptually a Unicode
/// string but is physically encoded as UTF-8 then percent-escaped so that
/// any byte outside the visible ASCII range (`0x20`–`0x7E`) plus `%`
/// itself is replaced with `%XX` (uppercase hex). Without this, header
/// libraries reject control characters and non-ASCII bytes.
pub fn encode_grpc_message(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    for &b in message.as_bytes() {
        if (0x20..=0x7E).contains(&b) && b != b'%' {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", b));
        }
    }
    out
}

/// Percent-decode a `grpc-message` header value.
///
/// Inverse of [`encode_grpc_message`]. Per spec, decoders MUST NOT error
/// on invalid escapes: any malformed `%XX` sequence is left as-is in the
/// output. The result is interpreted as UTF-8; invalid UTF-8 falls back
/// to the raw input.
pub fn decode_grpc_message(message: &str) -> String {
    let bytes = message.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    match String::from_utf8(out) {
        Ok(s) => s,
        Err(_) => message.to_string(),
    }
}

/// Parse a gRPC timeout header value into a [`Duration`](std::time::Duration).
///
/// The gRPC specification defines the `grpc-timeout` header with the format
/// `<number><unit>` where unit is one of:
///
/// - `H` — hours
/// - `M` — minutes
/// - `S` — seconds
/// - `m` — milliseconds
/// - `u` — microseconds
/// - `n` — nanoseconds
///
/// Returns `None` if the value is empty, the number cannot be parsed, or
/// the unit is unrecognized.
///
/// # Examples
///
/// ```
/// use typeway_grpc::status::parse_grpc_timeout;
/// use std::time::Duration;
///
/// assert_eq!(parse_grpc_timeout("30S"), Some(Duration::from_secs(30)));
/// assert_eq!(parse_grpc_timeout("500m"), Some(Duration::from_millis(500)));
/// assert_eq!(parse_grpc_timeout("1H"), Some(Duration::from_secs(3600)));
/// assert_eq!(parse_grpc_timeout(""), None);
/// assert_eq!(parse_grpc_timeout("abc"), None);
/// ```
pub fn parse_grpc_timeout(value: &str) -> Option<std::time::Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.len() < 2 {
        return None;
    }

    let (num_str, unit) = value.split_at(value.len() - 1);
    let num: u64 = num_str.parse().ok()?;

    match unit {
        "H" => num.checked_mul(3600).map(std::time::Duration::from_secs),
        "M" => num.checked_mul(60).map(std::time::Duration::from_secs),
        "S" => Some(std::time::Duration::from_secs(num)),
        "m" => Some(std::time::Duration::from_millis(num)),
        "u" => Some(std::time::Duration::from_micros(num)),
        "n" => Some(std::time::Duration::from_nanos(num)),
        _ => None,
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
    fn grpc_message_encode_passes_through_visible_ascii() {
        assert_eq!(encode_grpc_message("hello world"), "hello world");
        assert_eq!(encode_grpc_message("user not found"), "user not found");
    }

    #[test]
    fn grpc_message_encode_escapes_special_bytes() {
        assert_eq!(encode_grpc_message("\t"), "%09");
        assert_eq!(encode_grpc_message("\n"), "%0A");
        assert_eq!(encode_grpc_message("\r"), "%0D");
        assert_eq!(encode_grpc_message("%"), "%25");
        assert_eq!(encode_grpc_message("文"), "%E6%96%87");
    }

    #[test]
    fn grpc_message_round_trips() {
        let inputs = [
            "",
            "plain message",
            "\t\n\r",
            "unicode: 文字",
            "100% safe",
            "\t\n\r unicode: 文字",
        ];
        for s in &inputs {
            let encoded = encode_grpc_message(s);
            let decoded = decode_grpc_message(&encoded);
            assert_eq!(decoded, *s, "round trip failed for {s:?}");
        }
    }

    #[test]
    fn grpc_message_decode_leaves_invalid_escapes_alone() {
        assert_eq!(decode_grpc_message("%ZZ"), "%ZZ");
        assert_eq!(decode_grpc_message("100%"), "100%");
        assert_eq!(decode_grpc_message("%2"), "%2");
    }

    #[test]
    fn success_codes_map_to_ok() {
        assert_eq!(http_to_grpc_code(http::StatusCode::OK), GrpcCode::Ok);
        assert_eq!(http_to_grpc_code(http::StatusCode::CREATED), GrpcCode::Ok);
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
        assert_eq!(http_to_grpc_code(http::StatusCode::GONE), GrpcCode::Unknown);
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

    #[test]
    fn grpc_code_from_i32() {
        assert_eq!(GrpcCode::from_i32(0), GrpcCode::Ok);
        assert_eq!(GrpcCode::from_i32(1), GrpcCode::Cancelled);
        assert_eq!(GrpcCode::from_i32(2), GrpcCode::Unknown);
        assert_eq!(GrpcCode::from_i32(3), GrpcCode::InvalidArgument);
        assert_eq!(GrpcCode::from_i32(4), GrpcCode::DeadlineExceeded);
        assert_eq!(GrpcCode::from_i32(5), GrpcCode::NotFound);
        assert_eq!(GrpcCode::from_i32(6), GrpcCode::AlreadyExists);
        assert_eq!(GrpcCode::from_i32(7), GrpcCode::PermissionDenied);
        assert_eq!(GrpcCode::from_i32(8), GrpcCode::ResourceExhausted);
        assert_eq!(GrpcCode::from_i32(9), GrpcCode::FailedPrecondition);
        assert_eq!(GrpcCode::from_i32(10), GrpcCode::Aborted);
        assert_eq!(GrpcCode::from_i32(11), GrpcCode::OutOfRange);
        assert_eq!(GrpcCode::from_i32(12), GrpcCode::Unimplemented);
        assert_eq!(GrpcCode::from_i32(13), GrpcCode::Internal);
        assert_eq!(GrpcCode::from_i32(14), GrpcCode::Unavailable);
        assert_eq!(GrpcCode::from_i32(15), GrpcCode::DataLoss);
        assert_eq!(GrpcCode::from_i32(16), GrpcCode::Unauthenticated);
    }

    #[test]
    fn grpc_code_from_i32_unknown_values() {
        assert_eq!(GrpcCode::from_i32(-1), GrpcCode::Unknown);
        assert_eq!(GrpcCode::from_i32(99), GrpcCode::Unknown);
        assert_eq!(GrpcCode::from_i32(999), GrpcCode::Unknown);
    }

    #[test]
    fn grpc_code_roundtrip() {
        let codes = [
            GrpcCode::Ok,
            GrpcCode::Cancelled,
            GrpcCode::Unknown,
            GrpcCode::InvalidArgument,
            GrpcCode::DeadlineExceeded,
            GrpcCode::NotFound,
            GrpcCode::AlreadyExists,
            GrpcCode::PermissionDenied,
            GrpcCode::ResourceExhausted,
            GrpcCode::Unimplemented,
            GrpcCode::Internal,
            GrpcCode::Unavailable,
            GrpcCode::Unauthenticated,
        ];
        for code in codes {
            assert_eq!(GrpcCode::from_i32(code.as_i32()), code);
        }
    }

    #[test]
    fn parse_grpc_timeout_seconds() {
        assert_eq!(
            parse_grpc_timeout("30S"),
            Some(std::time::Duration::from_secs(30))
        );
        assert_eq!(
            parse_grpc_timeout("1S"),
            Some(std::time::Duration::from_secs(1))
        );
        assert_eq!(
            parse_grpc_timeout("0S"),
            Some(std::time::Duration::from_secs(0))
        );
    }

    #[test]
    fn parse_grpc_timeout_milliseconds() {
        assert_eq!(
            parse_grpc_timeout("500m"),
            Some(std::time::Duration::from_millis(500))
        );
        assert_eq!(
            parse_grpc_timeout("1m"),
            Some(std::time::Duration::from_millis(1))
        );
    }

    #[test]
    fn parse_grpc_timeout_hours() {
        assert_eq!(
            parse_grpc_timeout("1H"),
            Some(std::time::Duration::from_secs(3600))
        );
        assert_eq!(
            parse_grpc_timeout("2H"),
            Some(std::time::Duration::from_secs(7200))
        );
    }

    #[test]
    fn parse_grpc_timeout_minutes() {
        assert_eq!(
            parse_grpc_timeout("5M"),
            Some(std::time::Duration::from_secs(300))
        );
    }

    #[test]
    fn parse_grpc_timeout_microseconds() {
        assert_eq!(
            parse_grpc_timeout("100u"),
            Some(std::time::Duration::from_micros(100))
        );
    }

    #[test]
    fn parse_grpc_timeout_nanoseconds() {
        assert_eq!(
            parse_grpc_timeout("1000n"),
            Some(std::time::Duration::from_nanos(1000))
        );
    }

    #[test]
    fn parse_grpc_timeout_invalid() {
        assert_eq!(parse_grpc_timeout(""), None);
        assert_eq!(parse_grpc_timeout("S"), None);
        assert_eq!(parse_grpc_timeout("abc"), None);
        assert_eq!(parse_grpc_timeout("30x"), None);
        assert_eq!(parse_grpc_timeout("  "), None);
    }

    #[test]
    fn parse_grpc_timeout_overflow_returns_none() {
        // u64::MAX * 3600 would overflow — should return None, not panic.
        assert_eq!(parse_grpc_timeout(&format!("{}H", u64::MAX)), None);
        assert_eq!(parse_grpc_timeout(&format!("{}M", u64::MAX)), None);
    }
}
