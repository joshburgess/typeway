//! Content negotiation type-level primitives.
//!
//! [`Negotiated`] is a type-level marker that declares a response supports
//! multiple content types. The framework inspects the `Accept` header and
//! picks the best matching format at runtime.
//!
//! # Example
//!
//! ```ignore
//! use typeway_core::negotiate::{Negotiated, ContentFormat};
//!
//! // A response that can be rendered as JSON or plain text.
//! type UserResponse = Negotiated<User, (JsonFormat, TextFormat)>;
//! ```

use std::marker::PhantomData;

/// A response that supports multiple content types via negotiation.
///
/// `T` is the domain type returned by the handler. `Formats` is a tuple
/// of format marker types (e.g., `(JsonFormat, TextFormat)`) that the
/// framework can negotiate between based on the `Accept` header.
///
/// The handler returns the domain value `T` directly, and the framework
/// wraps it in the best matching format.
pub struct Negotiated<T, Formats = ()> {
    _marker: PhantomData<(T, Formats)>,
}

/// Trait implemented by format marker types that represent a content type.
///
/// Each format declares its MIME type and an optional quality weight for
/// tie-breaking when the client sends `Accept: */*`.
///
/// # Example
///
/// ```
/// use typeway_core::negotiate::ContentFormat;
///
/// struct MyCustomFormat;
///
/// impl ContentFormat for MyCustomFormat {
///     const CONTENT_TYPE: &'static str = "application/x-custom";
/// }
/// ```
pub trait ContentFormat {
    /// The MIME type this format produces (e.g., `"application/json"`).
    const CONTENT_TYPE: &'static str;

    /// Quality weight for tie-breaking when the client accepts multiple
    /// formats equally (0.0 - 1.0). Higher values are preferred.
    ///
    /// Defaults to `1.0`. When two formats both match `*/*`, the one
    /// with the higher quality weight is chosen. If equal, the first
    /// format in the tuple wins.
    const DEFAULT_QUALITY: f32 = 1.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestJson;
    impl ContentFormat for TestJson {
        const CONTENT_TYPE: &'static str = "application/json";
    }

    struct TestText;
    impl ContentFormat for TestText {
        const CONTENT_TYPE: &'static str = "text/plain";
        const DEFAULT_QUALITY: f32 = 0.9;
    }

    #[test]
    fn content_format_constants() {
        assert_eq!(TestJson::CONTENT_TYPE, "application/json");
        assert_eq!(TestText::CONTENT_TYPE, "text/plain");
        assert!((TestJson::DEFAULT_QUALITY - 1.0).abs() < f32::EPSILON);
        assert!((TestText::DEFAULT_QUALITY - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn negotiated_is_zero_sized() {
        assert_eq!(
            std::mem::size_of::<Negotiated<String, (TestJson, TestText)>>(),
            0
        );
    }
}
