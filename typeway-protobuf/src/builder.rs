//! Typestate builders for compile-time enforced message construction.
//!
//! A typestate builder tracks which required fields have been set at the type
//! level. The `.build()` method is only available when all required fields
//! are set — attempting to build an incomplete message is a compile error.
//!
//! # Example
//!
//! ```ignore
//! #[derive(TypewayCodec, TypestateBuilder)]
//! struct User {
//!     #[proto(tag = 1)]
//!     #[required]
//!     id: u32,
//!     #[proto(tag = 2)]
//!     #[required]
//!     name: String,
//!     #[proto(tag = 3)]
//!     email: Option<String>,
//! }
//!
//! // Compiles: all required fields set
//! let user = User::builder()
//!     .id(42)
//!     .name("Alice".into())
//!     .email("alice@example.com".into())
//!     .build();
//!
//! // Compile error: `name` not set
//! let user = User::builder()
//!     .id(42)
//!     .build(); // ERROR: method `build` not found
//! ```

/// Marker type: a required field has not yet been set.
pub struct Missing;

/// Marker type: a required field has been set.
pub struct Set;

/// Trait for types that have a typestate builder.
///
/// Implemented by `#[derive(TypestateBuilder)]` (when available).
/// Can also be implemented manually.
pub trait HasBuilder: Sized {
    /// The initial builder type (all required fields `Missing`).
    type Builder;

    /// Create a new builder with all optional fields defaulted
    /// and all required fields unset.
    fn builder() -> Self::Builder;
}
