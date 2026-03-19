//! Integration tests for content negotiation.

use http::header::CONTENT_TYPE;
use http::StatusCode;
use typeway_server::negotiate::*;
use typeway_server::response::IntoResponse;

#[derive(serde::Serialize, Clone)]
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
        id: 42,
        name: "Bob".to_string(),
    }
}

#[test]
fn json_returned_when_accept_is_application_json() {
    let user = test_user();
    let resp: NegotiatedResponse<TestUser, (JsonFormat, TextFormat)> =
        NegotiatedResponse::new(user, Some("application/json".to_string()));
    let http_resp = resp.into_response();

    assert_eq!(http_resp.status(), StatusCode::OK);
    assert_eq!(
        http_resp.headers().get(CONTENT_TYPE).unwrap(),
        "application/json"
    );
}

#[test]
fn text_returned_when_accept_is_text_plain() {
    let user = test_user();
    let resp: NegotiatedResponse<TestUser, (JsonFormat, TextFormat)> =
        NegotiatedResponse::new(user, Some("text/plain".to_string()));
    let http_resp = resp.into_response();

    assert_eq!(http_resp.status(), StatusCode::OK);
    assert_eq!(
        http_resp.headers().get(CONTENT_TYPE).unwrap(),
        "text/plain; charset=utf-8"
    );
}

#[test]
fn json_is_default_when_accept_is_wildcard() {
    let user = test_user();
    let resp: NegotiatedResponse<TestUser, (JsonFormat, TextFormat)> =
        NegotiatedResponse::new(user, Some("*/*".to_string()));
    let http_resp = resp.into_response();

    assert_eq!(http_resp.status(), StatusCode::OK);
    assert_eq!(
        http_resp.headers().get(CONTENT_TYPE).unwrap(),
        "application/json"
    );
}

#[test]
fn json_is_default_when_no_accept_header() {
    let user = test_user();
    let resp: NegotiatedResponse<TestUser, (JsonFormat, TextFormat)> =
        NegotiatedResponse::new(user, None);
    let http_resp = resp.into_response();

    assert_eq!(http_resp.status(), StatusCode::OK);
    assert_eq!(
        http_resp.headers().get(CONTENT_TYPE).unwrap(),
        "application/json"
    );
}

#[test]
fn correct_content_type_header_is_set() {
    // Verify that the Content-Type header exactly matches the format's declared type.
    let user = test_user();

    // JSON
    let resp: NegotiatedResponse<TestUser, (JsonFormat, TextFormat)> =
        NegotiatedResponse::new(user.clone(), Some("application/json".to_string()));
    let http_resp = resp.into_response();
    assert_eq!(
        http_resp
            .headers()
            .get(CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "application/json"
    );

    // Text
    let resp: NegotiatedResponse<TestUser, (JsonFormat, TextFormat)> =
        NegotiatedResponse::new(user, Some("text/plain".to_string()));
    let http_resp = resp.into_response();
    assert_eq!(
        http_resp
            .headers()
            .get(CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "text/plain; charset=utf-8"
    );
}

#[test]
fn quality_values_respected() {
    // When text/plain has higher quality than application/json, text should win.
    let user = test_user();
    let resp: NegotiatedResponse<TestUser, (JsonFormat, TextFormat)> = NegotiatedResponse::new(
        user,
        Some("application/json;q=0.5, text/plain;q=0.9".to_string()),
    );
    let http_resp = resp.into_response();

    assert_eq!(
        http_resp.headers().get(CONTENT_TYPE).unwrap(),
        "text/plain; charset=utf-8"
    );
}

#[test]
fn unknown_accept_falls_back_to_default() {
    // If the client requests a format we don't support, fall back to first format.
    let user = test_user();
    let resp: NegotiatedResponse<TestUser, (JsonFormat, TextFormat)> =
        NegotiatedResponse::new(user, Some("application/xml".to_string()));
    let http_resp = resp.into_response();

    assert_eq!(http_resp.status(), StatusCode::OK);
    assert_eq!(
        http_resp.headers().get(CONTENT_TYPE).unwrap(),
        "application/json"
    );
}

#[test]
fn accept_header_extractor() {
    // Test the AcceptHeader extractor directly.
    use typeway_server::extract::FromRequestParts;

    let builder = http::Request::builder()
        .header(http::header::ACCEPT, "text/plain")
        .body(())
        .unwrap();
    let (parts, _) = builder.into_parts();

    let accept = AcceptHeader::from_request_parts(&parts).unwrap();
    assert_eq!(accept.0, Some("text/plain".to_string()));

    // Without Accept header
    let req = http::Request::builder().body(()).unwrap();
    let (parts, _) = req.into_parts();
    let accept = AcceptHeader::from_request_parts(&parts).unwrap();
    assert_eq!(accept.0, None);
}
