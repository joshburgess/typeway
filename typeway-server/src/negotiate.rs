//! Content negotiation engine and format wrappers.
//!
//! Provides format marker types ([`JsonFormat`], [`TextFormat`], [`HtmlFormat`],
//! [`CsvFormat`]), the [`RenderAs`] trait for converting domain types into
//! specific formats, and [`NegotiatedResponse`] which inspects the `Accept`
//! header to pick the best representation.
//!
//! # Example
//!
//! ```ignore
//! use typeway_server::negotiate::*;
//!
//! #[derive(serde::Serialize)]
//! struct User { id: u32, name: String }
//!
//! impl std::fmt::Display for User {
//!     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//!         write!(f, "User({}, {})", self.id, self.name)
//!     }
//! }
//!
//! async fn get_user(accept: AcceptHeader) -> NegotiatedResponse<User, (JsonFormat, TextFormat)> {
//!     NegotiatedResponse::new(User { id: 1, name: "Alice".into() }, accept.0)
//! }
//! ```

use http::header::CONTENT_TYPE;
use http::StatusCode;
use typeway_core::negotiate::ContentFormat;

use crate::body::{body_from_bytes, body_from_string, BoxBody};
use crate::response::IntoResponse;

// ---------------------------------------------------------------------------
// Format marker types
// ---------------------------------------------------------------------------

/// JSON representation. Serializes via `serde_json`.
pub struct JsonFormat;

impl ContentFormat for JsonFormat {
    const CONTENT_TYPE: &'static str = "application/json";
}

/// Plain text representation. Uses `Display`.
pub struct TextFormat;

impl ContentFormat for TextFormat {
    const CONTENT_TYPE: &'static str = "text/plain; charset=utf-8";
}

/// HTML representation. Uses `Display` (intended for types that produce HTML).
pub struct HtmlFormat;

impl ContentFormat for HtmlFormat {
    const CONTENT_TYPE: &'static str = "text/html; charset=utf-8";
}

/// CSV representation.
pub struct CsvFormat;

impl ContentFormat for CsvFormat {
    const CONTENT_TYPE: &'static str = "text/csv";
}

/// XML representation. Requires explicit [`RenderAsXml`] impls per type.
pub struct XmlFormat;

impl ContentFormat for XmlFormat {
    const CONTENT_TYPE: &'static str = "application/xml";
}

// ---------------------------------------------------------------------------
// RenderAsXml trait
// ---------------------------------------------------------------------------

/// Trait for types that can render as XML.
/// Unlike JsonFormat/TextFormat which have blanket impls, XML rendering
/// requires an explicit impl per type since there's no standard XML serialization trait.
pub trait RenderAsXml {
    fn to_xml(&self) -> String;
}

impl<T: RenderAsXml> RenderAs<XmlFormat> for T {
    fn render(&self) -> Result<(Vec<u8>, &'static str), String> {
        Ok((self.to_xml().into_bytes(), XmlFormat::CONTENT_TYPE))
    }
}

// ---------------------------------------------------------------------------
// RenderAs trait
// ---------------------------------------------------------------------------

/// Convert a domain type into bytes for a specific content format.
///
/// Implement this trait to teach the negotiation engine how to serialize
/// your type as a particular format.
///
/// Blanket implementations are provided for:
/// - `RenderAs<JsonFormat>` for any `T: serde::Serialize`
/// - `RenderAs<TextFormat>` for any `T: Display`
pub trait RenderAs<Format: ContentFormat> {
    /// Render this value into bytes and its content-type string.
    fn render(&self) -> Result<(Vec<u8>, &'static str), String>;
}

impl<T: serde::Serialize> RenderAs<JsonFormat> for T {
    fn render(&self) -> Result<(Vec<u8>, &'static str), String> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        Ok((bytes, JsonFormat::CONTENT_TYPE))
    }
}

impl<T: std::fmt::Display> RenderAs<TextFormat> for T {
    fn render(&self) -> Result<(Vec<u8>, &'static str), String> {
        Ok((self.to_string().into_bytes(), TextFormat::CONTENT_TYPE))
    }
}

// ---------------------------------------------------------------------------
// NegotiateFormats trait
// ---------------------------------------------------------------------------

/// Select the best format from a tuple of formats based on the `Accept` header
/// and render the domain value.
///
/// Implemented for format tuples of arities 1 through 6 via macro.
pub trait NegotiateFormats<T> {
    /// All supported content types, in preference order.
    fn supported_types() -> Vec<&'static str>;

    /// Pick the best format for the given `Accept` header and render `value`.
    fn negotiate_and_render(
        value: &T,
        accept: Option<&str>,
    ) -> Result<(Vec<u8>, &'static str), String>;
}

/// Parse an `Accept` header value into a list of (media_type, quality) pairs,
/// sorted by quality descending.
fn parse_accept(accept: &str) -> Vec<(&str, f32)> {
    let mut entries: Vec<(&str, f32)> = accept
        .split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            let mut parts = entry.splitn(2, ';');
            let media_type = parts.next()?.trim();
            let quality = parts
                .next()
                .and_then(|params| {
                    params.split(';').find_map(|p| {
                        let p = p.trim();
                        p.strip_prefix("q=")
                            .and_then(|q| q.trim().parse::<f32>().ok())
                    })
                })
                .unwrap_or(1.0);
            Some((media_type, quality))
        })
        .collect();
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    entries
}

/// Check whether a media type from the Accept header matches a supported
/// content type. Supports wildcard matching (`*/*`, `application/*`).
fn media_type_matches(accept_type: &str, supported: &str) -> bool {
    if accept_type == "*/*" {
        return true;
    }
    // Extract just the media type part (before any parameters like charset)
    let supported_base = supported.split(';').next().unwrap_or(supported).trim();
    if accept_type == supported_base {
        return true;
    }
    // Check type/* wildcard (e.g. "text/*" matches "text/plain")
    if let Some(prefix) = accept_type.strip_suffix("/*") {
        if let Some(sup_prefix) = supported_base.split('/').next() {
            return prefix == sup_prefix;
        }
    }
    false
}

/// Implementation helper: given an Accept header and a list of supported types
/// (in preference order), return the index of the best matching type.
fn best_match(accept: Option<&str>, supported: &[&str]) -> usize {
    let accept = match accept {
        Some(a) if !a.is_empty() => a,
        _ => return 0, // No Accept header -> use first (default) format
    };

    let entries = parse_accept(accept);

    // For each Accept entry (sorted by quality), find the first supported type
    // that matches.
    for (media_type, _quality) in &entries {
        for (idx, supported_type) in supported.iter().enumerate() {
            if media_type_matches(media_type, supported_type) {
                return idx;
            }
        }
    }

    // No match found -> default to first format
    0
}

// Generate NegotiateFormats impls for tuples of arities 1-6.
macro_rules! impl_negotiate_formats {
    // Single format
    ([$F1:ident], [$idx1:tt]) => {
        impl<T, $F1> NegotiateFormats<T> for ($F1,)
        where
            $F1: ContentFormat,
            T: RenderAs<$F1>,
        {
            fn supported_types() -> Vec<&'static str> {
                vec![$F1::CONTENT_TYPE]
            }

            fn negotiate_and_render(
                value: &T,
                _accept: Option<&str>,
            ) -> Result<(Vec<u8>, &'static str), String> {
                <T as RenderAs<$F1>>::render(value)
            }
        }
    };
    // Multiple formats
    ([$F1:ident $(, $FN:ident)*], [$idx1:tt $(, $idxN:tt)*]) => {
        impl<T, $F1 $(, $FN)*> NegotiateFormats<T> for ($F1, $($FN,)*)
        where
            $F1: ContentFormat,
            $($FN: ContentFormat,)*
            T: RenderAs<$F1> $(+ RenderAs<$FN>)*,
        {
            fn supported_types() -> Vec<&'static str> {
                vec![$F1::CONTENT_TYPE $(, $FN::CONTENT_TYPE)*]
            }

            fn negotiate_and_render(
                value: &T,
                accept: Option<&str>,
            ) -> Result<(Vec<u8>, &'static str), String> {
                let supported = [$F1::CONTENT_TYPE $(, $FN::CONTENT_TYPE)*];
                let idx = best_match(accept, &supported);
                // Dispatch to the correct RenderAs impl based on index.
                let renderers: Vec<Box<dyn Fn(&T) -> Result<(Vec<u8>, &'static str), String>>> = vec![
                    Box::new(|v| <T as RenderAs<$F1>>::render(v)),
                    $(Box::new(|v| <T as RenderAs<$FN>>::render(v)),)*
                ];
                (renderers[idx])(value)
            }
        }
    };
}

impl_negotiate_formats!([F1], [0]);
impl_negotiate_formats!([F1, F2], [0, 1]);
impl_negotiate_formats!([F1, F2, F3], [0, 1, 2]);
impl_negotiate_formats!([F1, F2, F3, F4], [0, 1, 2, 3]);
impl_negotiate_formats!([F1, F2, F3, F4, F5], [0, 1, 2, 3, 4]);
impl_negotiate_formats!([F1, F2, F3, F4, F5, F6], [0, 1, 2, 3, 4, 5]);

// ---------------------------------------------------------------------------
// NegotiatedResponse
// ---------------------------------------------------------------------------

/// A response that holds a domain value and negotiates the best content type
/// based on the `Accept` header.
///
/// `T` is the domain type, `Formats` is a tuple of format markers
/// (e.g., `(JsonFormat, TextFormat)`).
///
/// Implements [`IntoResponse`] when `Formats: NegotiateFormats<T>`.
pub struct NegotiatedResponse<T, Formats> {
    value: T,
    accept: Option<String>,
    _formats: std::marker::PhantomData<Formats>,
}

impl<T, Formats> NegotiatedResponse<T, Formats> {
    /// Create a new negotiated response.
    ///
    /// `accept` should be the value of the `Accept` header from the request,
    /// or `None` if absent. Use the [`AcceptHeader`] extractor to obtain this.
    pub fn new(value: T, accept: Option<String>) -> Self {
        NegotiatedResponse {
            value,
            accept,
            _formats: std::marker::PhantomData,
        }
    }
}

impl<T, Formats> IntoResponse for NegotiatedResponse<T, Formats>
where
    Formats: NegotiateFormats<T>,
{
    fn into_response(self) -> http::Response<BoxBody> {
        match Formats::negotiate_and_render(&self.value, self.accept.as_deref()) {
            Ok((body_bytes, content_type)) => {
                let body = body_from_bytes(bytes::Bytes::from(body_bytes));
                let mut res = http::Response::new(body);
                if let Ok(val) = http::HeaderValue::from_str(content_type) {
                    res.headers_mut().insert(CONTENT_TYPE, val);
                }
                res
            }
            Err(e) => {
                let mut res =
                    http::Response::new(body_from_string(format!("negotiation error: {e}")));
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                res
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AcceptHeader extractor
// ---------------------------------------------------------------------------

/// Extracts the `Accept` header value from the request.
///
/// Use this in handler arguments to pass into [`NegotiatedResponse::new`].
///
/// # Example
///
/// ```ignore
/// use typeway_server::negotiate::*;
///
/// async fn handler(accept: AcceptHeader) -> NegotiatedResponse<MyType, (JsonFormat, TextFormat)> {
///     NegotiatedResponse::new(my_value, accept.0)
/// }
/// ```
pub struct AcceptHeader(pub Option<String>);

impl crate::extract::FromRequestParts for AcceptHeader {
    type Error = std::convert::Infallible;

    fn from_request_parts(parts: &http::request::Parts) -> Result<Self, Self::Error> {
        let accept = parts
            .headers
            .get(http::header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        Ok(AcceptHeader(accept))
    }
}

// Infallible always succeeds, but we need IntoResponse for the trait bound.
impl IntoResponse for std::convert::Infallible {
    fn into_response(self) -> http::Response<BoxBody> {
        match self {}
    }
}

// ---------------------------------------------------------------------------
// Convenience function
// ---------------------------------------------------------------------------

/// Wrap a domain value for content negotiation with the given Accept header.
///
/// # Example
///
/// ```ignore
/// async fn get_user(accept: AcceptHeader) -> NegotiatedResponse<User, (JsonFormat, TextFormat)> {
///     negotiated(user, accept)
/// }
/// ```
pub fn negotiated<T, Formats>(value: T, accept: AcceptHeader) -> NegotiatedResponse<T, Formats> {
    NegotiatedResponse::new(value, accept.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize)]
    struct TestUser {
        id: u32,
        name: String,
    }

    impl std::fmt::Display for TestUser {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "User({}, {})", self.id, self.name)
        }
    }

    fn test_user() -> TestUser {
        TestUser {
            id: 1,
            name: "Alice".to_string(),
        }
    }

    #[test]
    fn parse_accept_simple() {
        let entries = parse_accept("application/json");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "application/json");
        assert!((entries[0].1 - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_accept_with_quality() {
        let entries = parse_accept("text/plain;q=0.5, application/json;q=0.9");
        assert_eq!(entries.len(), 2);
        // Sorted by quality descending
        assert_eq!(entries[0].0, "application/json");
        assert_eq!(entries[1].0, "text/plain");
    }

    #[test]
    fn parse_accept_wildcard() {
        let entries = parse_accept("*/*");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "*/*");
    }

    #[test]
    fn media_type_matches_exact() {
        assert!(media_type_matches("application/json", "application/json"));
        assert!(!media_type_matches("application/json", "text/plain"));
    }

    #[test]
    fn media_type_matches_with_params() {
        assert!(media_type_matches(
            "text/plain",
            "text/plain; charset=utf-8"
        ));
    }

    #[test]
    fn media_type_matches_wildcard() {
        assert!(media_type_matches("*/*", "application/json"));
        assert!(media_type_matches("text/*", "text/plain"));
        assert!(!media_type_matches("text/*", "application/json"));
    }

    #[test]
    fn best_match_no_accept() {
        let supported = &["application/json", "text/plain"];
        assert_eq!(best_match(None, supported), 0);
    }

    #[test]
    fn best_match_wildcard() {
        let supported = &["application/json", "text/plain"];
        assert_eq!(best_match(Some("*/*"), supported), 0);
    }

    #[test]
    fn best_match_specific() {
        let supported = &["application/json", "text/plain; charset=utf-8"];
        assert_eq!(best_match(Some("text/plain"), supported), 1);
    }

    #[test]
    fn best_match_quality_order() {
        let supported = &["application/json", "text/plain; charset=utf-8"];
        assert_eq!(
            best_match(
                Some("text/plain;q=0.9, application/json;q=0.5"),
                supported
            ),
            1
        );
    }

    #[test]
    fn render_as_json() {
        let user = test_user();
        let (bytes, ct) = <TestUser as RenderAs<JsonFormat>>::render(&user).unwrap();
        assert_eq!(ct, "application/json");
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed["name"], "Alice");
    }

    #[test]
    fn render_as_text() {
        let user = test_user();
        let (bytes, ct) = <TestUser as RenderAs<TextFormat>>::render(&user).unwrap();
        assert_eq!(ct, "text/plain; charset=utf-8");
        assert_eq!(String::from_utf8(bytes).unwrap(), "User(1, Alice)");
    }

    #[test]
    fn negotiate_json_when_accepted() {
        let user = test_user();
        let (bytes, ct) =
            <(JsonFormat, TextFormat) as NegotiateFormats<TestUser>>::negotiate_and_render(
                &user,
                Some("application/json"),
            )
            .unwrap();
        assert_eq!(ct, "application/json");
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed["id"], 1);
    }

    #[test]
    fn negotiate_text_when_accepted() {
        let user = test_user();
        let (bytes, ct) =
            <(JsonFormat, TextFormat) as NegotiateFormats<TestUser>>::negotiate_and_render(
                &user,
                Some("text/plain"),
            )
            .unwrap();
        assert_eq!(ct, "text/plain; charset=utf-8");
        assert_eq!(String::from_utf8(bytes).unwrap(), "User(1, Alice)");
    }

    #[test]
    fn negotiate_default_on_wildcard() {
        let user = test_user();
        let (_bytes, ct) =
            <(JsonFormat, TextFormat) as NegotiateFormats<TestUser>>::negotiate_and_render(
                &user,
                Some("*/*"),
            )
            .unwrap();
        // Default to first format (JSON)
        assert_eq!(ct, "application/json");
    }

    #[test]
    fn negotiate_default_on_no_accept() {
        let user = test_user();
        let (_bytes, ct) =
            <(JsonFormat, TextFormat) as NegotiateFormats<TestUser>>::negotiate_and_render(
                &user,
                None,
            )
            .unwrap();
        assert_eq!(ct, "application/json");
    }

    #[test]
    fn negotiated_response_into_response_json() {
        let user = test_user();
        let resp: NegotiatedResponse<TestUser, (JsonFormat, TextFormat)> =
            NegotiatedResponse::new(user, Some("application/json".to_string()));
        let http_resp = resp.into_response();
        assert_eq!(http_resp.status(), StatusCode::OK);
        assert_eq!(
            http_resp.headers().get("content-type").unwrap(),
            "application/json"
        );
    }

    #[test]
    fn negotiated_response_into_response_text() {
        let user = test_user();
        let resp: NegotiatedResponse<TestUser, (JsonFormat, TextFormat)> =
            NegotiatedResponse::new(user, Some("text/plain".to_string()));
        let http_resp = resp.into_response();
        assert_eq!(http_resp.status(), StatusCode::OK);
        assert_eq!(
            http_resp.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
    }

    #[test]
    fn single_format_tuple() {
        let user = test_user();
        let (_bytes, ct) =
            <(JsonFormat,) as NegotiateFormats<TestUser>>::negotiate_and_render(&user, None)
                .unwrap();
        assert_eq!(ct, "application/json");
    }

    #[test]
    fn three_format_tuple() {
        let user = test_user();
        let (_, ct) = <(JsonFormat, TextFormat, JsonFormat) as NegotiateFormats<
            TestUser,
        >>::negotiate_and_render(&user, Some("text/plain"))
        .unwrap();
        assert_eq!(ct, "text/plain; charset=utf-8");
    }

    #[test]
    fn supported_types_lists_all() {
        let types =
            <(JsonFormat, TextFormat) as NegotiateFormats<TestUser>>::supported_types();
        assert_eq!(types, vec!["application/json", "text/plain; charset=utf-8"]);
    }
}
