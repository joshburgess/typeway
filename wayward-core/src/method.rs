//! HTTP method types for type-level route encoding.
//!
//! Each HTTP method is a zero-sized type implementing [`HttpMethod`].
//! These are used as the `M` parameter in [`Endpoint<M, P, Req, Res>`](crate::endpoint::Endpoint).

/// Associates a zero-sized method type with its [`http::Method`] value.
pub trait HttpMethod {
    /// The runtime HTTP method value.
    const METHOD: http::Method;
}

/// HTTP GET method.
pub struct Get;

/// HTTP POST method.
pub struct Post;

/// HTTP PUT method.
pub struct Put;

/// HTTP DELETE method.
pub struct Delete;

/// HTTP PATCH method.
pub struct Patch;

/// HTTP HEAD method.
pub struct Head;

/// HTTP OPTIONS method.
pub struct Options;

impl HttpMethod for Get {
    const METHOD: http::Method = http::Method::GET;
}
impl HttpMethod for Post {
    const METHOD: http::Method = http::Method::POST;
}
impl HttpMethod for Put {
    const METHOD: http::Method = http::Method::PUT;
}
impl HttpMethod for Delete {
    const METHOD: http::Method = http::Method::DELETE;
}
impl HttpMethod for Patch {
    const METHOD: http::Method = http::Method::PATCH;
}
impl HttpMethod for Head {
    const METHOD: http::Method = http::Method::HEAD;
}
impl HttpMethod for Options {
    const METHOD: http::Method = http::Method::OPTIONS;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_values() {
        assert_eq!(Get::METHOD, http::Method::GET);
        assert_eq!(Post::METHOD, http::Method::POST);
        assert_eq!(Put::METHOD, http::Method::PUT);
        assert_eq!(Delete::METHOD, http::Method::DELETE);
        assert_eq!(Patch::METHOD, http::Method::PATCH);
        assert_eq!(Head::METHOD, http::Method::HEAD);
        assert_eq!(Options::METHOD, http::Method::OPTIONS);
    }
}
