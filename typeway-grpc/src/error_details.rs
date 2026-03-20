//! Structured gRPC error details following Google's richer error model.
//!
//! These types represent the standard error detail messages from
//! `google.rpc.error_details.proto`. They can be attached to a
//! [`GrpcStatus`](crate::GrpcStatus) to provide rich error information
//! beyond just a code and message.
//!
//! # Google's richer error model
//!
//! The standard gRPC `Status` includes only a code and message. Google's
//! richer error model extends this with a `details` field that can carry
//! typed payloads such as field-level validation errors ([`BadRequest`]),
//! retry timing ([`RetryInfo`]), debug metadata ([`DebugInfo`]), and
//! structured error classification ([`ErrorInfo`]).
//!
//! Since the typeway gRPC bridge uses JSON encoding, error details are
//! serialized as JSON with `@type` discriminator fields matching the
//! canonical `type.googleapis.com/google.rpc.*` type URLs.
//!
//! # Example
//!
//! ```
//! use typeway_grpc::error_details::{RichGrpcStatus, BadRequest, FieldViolation};
//! use typeway_grpc::GrpcCode;
//!
//! let status = RichGrpcStatus::new(GrpcCode::InvalidArgument, "validation failed")
//!     .with_bad_request(BadRequest {
//!         field_violations: vec![
//!             FieldViolation {
//!                 field: "email".to_string(),
//!                 description: "must contain @".to_string(),
//!             },
//!             FieldViolation {
//!                 field: "password".to_string(),
//!                 description: "must be at least 8 characters".to_string(),
//!             },
//!         ],
//!     });
//!
//! assert_eq!(status.code, 3);
//! assert_eq!(status.details.len(), 1);
//! ```

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// A rich gRPC error status with optional detail payloads.
///
/// This extends [`GrpcStatus`](crate::GrpcStatus) with structured details
/// following Google's `google.rpc.Status` model.
///
/// # Example
///
/// ```
/// use typeway_grpc::error_details::{RichGrpcStatus, BadRequest, FieldViolation};
/// use typeway_grpc::GrpcCode;
///
/// let status = RichGrpcStatus::new(GrpcCode::InvalidArgument, "validation failed")
///     .with_bad_request(BadRequest {
///         field_violations: vec![
///             FieldViolation {
///                 field: "email".to_string(),
///                 description: "must contain @".to_string(),
///             },
///         ],
///     });
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichGrpcStatus {
    /// The gRPC status code.
    pub code: i32,
    /// Human-readable error message.
    pub message: String,
    /// Structured error details.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<ErrorDetail>,
}

impl RichGrpcStatus {
    /// Create a new rich status with the given code and message.
    pub fn new(code: crate::GrpcCode, message: impl Into<String>) -> Self {
        RichGrpcStatus {
            code: code.as_i32(),
            message: message.into(),
            details: Vec::new(),
        }
    }

    /// Add a [`BadRequest`] detail (field validation errors).
    pub fn with_bad_request(mut self, bad_request: BadRequest) -> Self {
        self.details.push(ErrorDetail::BadRequest(bad_request));
        self
    }

    /// Add a [`RetryInfo`] detail.
    pub fn with_retry_info(mut self, retry_delay: Duration) -> Self {
        self.details.push(ErrorDetail::RetryInfo(RetryInfo {
            retry_delay_seconds: retry_delay.as_secs(),
            retry_delay_nanos: retry_delay.subsec_nanos(),
        }));
        self
    }

    /// Add a [`DebugInfo`] detail.
    pub fn with_debug_info(
        mut self,
        stack_entries: Vec<String>,
        detail: impl Into<String>,
    ) -> Self {
        self.details.push(ErrorDetail::DebugInfo(DebugInfo {
            stack_entries,
            detail: detail.into(),
        }));
        self
    }

    /// Add an [`ErrorInfo`] detail.
    pub fn with_error_info(
        mut self,
        reason: impl Into<String>,
        domain: impl Into<String>,
        metadata: HashMap<String, String>,
    ) -> Self {
        self.details.push(ErrorDetail::ErrorInfo(ErrorInfo {
            reason: reason.into(),
            domain: domain.into(),
            metadata,
        }));
        self
    }

    /// Add a [`QuotaFailure`] detail.
    pub fn with_quota_failure(mut self, violations: Vec<QuotaViolation>) -> Self {
        self.details
            .push(ErrorDetail::QuotaFailure(QuotaFailure { violations }));
        self
    }

    /// Add a [`PreconditionFailure`] detail.
    pub fn with_precondition_failure(mut self, violations: Vec<PreconditionViolation>) -> Self {
        self.details
            .push(ErrorDetail::PreconditionFailure(PreconditionFailure {
                violations,
            }));
        self
    }

    /// Add a [`ResourceInfo`] detail.
    pub fn with_resource_info(
        mut self,
        resource_type: impl Into<String>,
        resource_name: impl Into<String>,
        owner: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.details.push(ErrorDetail::ResourceInfo(ResourceInfo {
            resource_type: resource_type.into(),
            resource_name: resource_name.into(),
            owner: owner.into(),
            description: description.into(),
        }));
        self
    }

    /// Add a [`Help`] detail with links.
    pub fn with_help(mut self, links: Vec<HelpLink>) -> Self {
        self.details.push(ErrorDetail::Help(Help { links }));
        self
    }

    /// Add a [`LocalizedMessage`] detail.
    pub fn with_localized_message(
        mut self,
        locale: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        self.details
            .push(ErrorDetail::LocalizedMessage(LocalizedMessage {
                locale: locale.into(),
                message: message.into(),
            }));
        self
    }

    /// Serialize to JSON bytes for inclusion in a gRPC response body.
    pub fn to_json_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Convert to response headers and body for the gRPC bridge.
    ///
    /// Returns a tuple of `(headers, body)` where headers are
    /// `(name, value)` pairs and body is the JSON-serialized details
    /// (empty if there are no details).
    pub fn to_grpc_response_parts(&self) -> (Vec<(String, String)>, Vec<u8>) {
        let headers = vec![
            ("grpc-status".to_string(), self.code.to_string()),
            ("grpc-message".to_string(), self.message.clone()),
        ];
        let body = if self.details.is_empty() {
            Vec::new()
        } else {
            self.to_json_bytes()
        };
        (headers, body)
    }
}

/// A typed error detail payload.
///
/// Each variant corresponds to a standard Google error detail type.
/// The `@type` field in the JSON representation uses the canonical
/// `type.googleapis.com/google.rpc.*` type URLs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "@type")]
pub enum ErrorDetail {
    /// Field-level validation errors.
    #[serde(rename = "type.googleapis.com/google.rpc.BadRequest")]
    BadRequest(BadRequest),
    /// When the client should retry.
    #[serde(rename = "type.googleapis.com/google.rpc.RetryInfo")]
    RetryInfo(RetryInfo),
    /// Debug information (not for end users).
    #[serde(rename = "type.googleapis.com/google.rpc.DebugInfo")]
    DebugInfo(DebugInfo),
    /// Structured error classification.
    #[serde(rename = "type.googleapis.com/google.rpc.ErrorInfo")]
    ErrorInfo(ErrorInfo),
    /// Quota/rate limit violation details.
    #[serde(rename = "type.googleapis.com/google.rpc.QuotaFailure")]
    QuotaFailure(QuotaFailure),
    /// Precondition failure details.
    #[serde(rename = "type.googleapis.com/google.rpc.PreconditionFailure")]
    PreconditionFailure(PreconditionFailure),
    /// Resource information for NOT_FOUND / ALREADY_EXISTS errors.
    #[serde(rename = "type.googleapis.com/google.rpc.ResourceInfo")]
    ResourceInfo(ResourceInfo),
    /// Help links.
    #[serde(rename = "type.googleapis.com/google.rpc.Help")]
    Help(Help),
    /// Localized error message.
    #[serde(rename = "type.googleapis.com/google.rpc.LocalizedMessage")]
    LocalizedMessage(LocalizedMessage),
}

/// Describes field-level validation errors.
///
/// Typically attached to a status with code [`InvalidArgument`](crate::GrpcCode::InvalidArgument).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BadRequest {
    /// Individual field violations.
    pub field_violations: Vec<FieldViolation>,
}

/// A single field-level validation error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldViolation {
    /// The field path (e.g., `"email"` or `"address.zip_code"`).
    pub field: String,
    /// A human-readable description of the violation.
    pub description: String,
}

/// Retry information for the client.
///
/// Typically attached to a status with code
/// [`Unavailable`](crate::GrpcCode::Unavailable) or
/// [`ResourceExhausted`](crate::GrpcCode::ResourceExhausted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryInfo {
    /// Seconds component of the retry delay.
    pub retry_delay_seconds: u64,
    /// Nanoseconds component of the retry delay.
    pub retry_delay_nanos: u32,
}

/// Debug information intended for developers, not end users.
///
/// Typically attached to a status with code [`Internal`](crate::GrpcCode::Internal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugInfo {
    /// Stack trace entries (most recent first).
    pub stack_entries: Vec<String>,
    /// Additional debug detail.
    pub detail: String,
}

/// Structured error classification with reason, domain, and metadata.
///
/// Useful for programmatic error handling where the client needs to
/// distinguish between different error conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    /// The reason for the error (e.g., `"RATE_LIMIT_EXCEEDED"`).
    pub reason: String,
    /// The logical grouping (e.g., `"googleapis.com"`).
    pub domain: String,
    /// Additional structured metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

/// Quota/rate limit violation details.
///
/// Typically attached to a status with code
/// [`ResourceExhausted`](crate::GrpcCode::ResourceExhausted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaFailure {
    /// Individual quota violations.
    pub violations: Vec<QuotaViolation>,
}

/// A single quota violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaViolation {
    /// The subject on which the quota check failed (e.g., `"project:my-project"`).
    pub subject: String,
    /// A human-readable description of the violation.
    pub description: String,
}

/// Precondition failure details.
///
/// Typically attached to a status with code
/// [`FailedPrecondition`](crate::GrpcCode::InvalidArgument) (gRPC code 9,
/// though this crate uses `InvalidArgument` as the closest available code).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreconditionFailure {
    /// Individual precondition violations.
    pub violations: Vec<PreconditionViolation>,
}

/// A single precondition violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreconditionViolation {
    /// The type of precondition that failed (e.g., `"TOS"`).
    #[serde(rename = "type")]
    pub violation_type: String,
    /// The subject on which the precondition was checked.
    pub subject: String,
    /// A human-readable description.
    pub description: String,
}

/// Resource information, typically for NOT_FOUND or ALREADY_EXISTS errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    /// The type of resource (e.g., `"user"`, `"project"`).
    pub resource_type: String,
    /// The name or identifier of the resource.
    pub resource_name: String,
    /// The owner of the resource (may be empty).
    pub owner: String,
    /// A human-readable description.
    pub description: String,
}

/// Help links pointing to documentation or troubleshooting resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Help {
    /// Related links.
    pub links: Vec<HelpLink>,
}

/// A single help link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelpLink {
    /// A human-readable description of the link.
    pub description: String,
    /// The URL.
    pub url: String,
}

/// A localized error message.
///
/// Allows returning error messages in the user's preferred language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalizedMessage {
    /// The locale (e.g., `"en-US"`, `"fr-FR"`).
    pub locale: String,
    /// The localized message text.
    pub message: String,
}

/// Trait for error types that can provide structured gRPC error details.
///
/// Implement this on your error types to include field-level validation
/// errors, retry info, or debug metadata in gRPC responses. This is
/// separate from [`IntoGrpcStatus`](crate::IntoGrpcStatus) to allow
/// opt-in rich error details without affecting types that only need
/// basic status information.
///
/// # Example
///
/// ```
/// use typeway_grpc::error_details::{
///     IntoRichGrpcStatus, RichGrpcStatus, BadRequest, FieldViolation,
/// };
/// use typeway_grpc::GrpcCode;
///
/// struct ValidationError {
///     fields: Vec<(String, String)>,
/// }
///
/// impl IntoRichGrpcStatus for ValidationError {
///     fn into_rich_grpc_status(&self) -> RichGrpcStatus {
///         RichGrpcStatus::new(GrpcCode::InvalidArgument, "validation failed")
///             .with_bad_request(BadRequest {
///                 field_violations: self.fields.iter().map(|(f, d)| {
///                     FieldViolation {
///                         field: f.clone(),
///                         description: d.clone(),
///                     }
///                 }).collect(),
///             })
///     }
/// }
/// ```
#[allow(clippy::wrong_self_convention)]
pub trait IntoRichGrpcStatus {
    /// Convert this value into a [`RichGrpcStatus`].
    ///
    /// Takes `&self` rather than `self` because error values are often
    /// borrowed (e.g., logged before conversion).
    fn into_rich_grpc_status(&self) -> RichGrpcStatus;
}
