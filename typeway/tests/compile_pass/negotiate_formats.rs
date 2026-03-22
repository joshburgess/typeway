// Content negotiation types compile with custom formats.

use typeway_core::negotiate::{ContentFormat, Negotiated};

struct JsonFormat;
impl ContentFormat for JsonFormat {
    const CONTENT_TYPE: &'static str = "application/json";
}

struct TextFormat;
impl ContentFormat for TextFormat {
    const CONTENT_TYPE: &'static str = "text/plain";
    const DEFAULT_QUALITY: f32 = 0.9;
}

struct XmlFormat;
impl ContentFormat for XmlFormat {
    const CONTENT_TYPE: &'static str = "application/xml";
    const DEFAULT_QUALITY: f32 = 0.5;
}

#[derive(serde::Serialize)]
struct User { name: String }

// Negotiated with two formats.
type _NegotiatedUser = Negotiated<User, (JsonFormat, TextFormat)>;

// Negotiated with three formats.
type _NegotiatedUserMulti = Negotiated<User, (JsonFormat, TextFormat, XmlFormat)>;

// Verify constants at compile time.
const _: () = {
    assert!(JsonFormat::DEFAULT_QUALITY as u32 == 1);
    assert!(TextFormat::CONTENT_TYPE.len() == 10); // "text/plain"
};

fn main() {}
