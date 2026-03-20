//! Tests for the gRPC multiplexer content-type detection logic.

use typeway_grpc::multiplex::is_grpc_request;

#[test]
fn application_grpc_is_detected() {
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "application/grpc")
        .body(())
        .unwrap();
    assert!(is_grpc_request(&req));
}

#[test]
fn application_grpc_json_is_detected() {
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "application/grpc+json")
        .body(())
        .unwrap();
    assert!(is_grpc_request(&req));
}

#[test]
fn application_grpc_proto_is_detected() {
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "application/grpc+proto")
        .body(())
        .unwrap();
    assert!(is_grpc_request(&req));
}

#[test]
fn application_json_is_not_grpc() {
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(())
        .unwrap();
    assert!(!is_grpc_request(&req));
}

#[test]
fn text_html_is_not_grpc() {
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "text/html")
        .body(())
        .unwrap();
    assert!(!is_grpc_request(&req));
}

#[test]
fn no_content_type_is_not_grpc() {
    let req = http::Request::builder().body(()).unwrap();
    assert!(!is_grpc_request(&req));
}

#[test]
fn empty_content_type_is_not_grpc() {
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "")
        .body(())
        .unwrap();
    assert!(!is_grpc_request(&req));
}

#[test]
fn partial_match_not_grpc() {
    // "application/grp" is not a valid gRPC content-type
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "application/grp")
        .body(())
        .unwrap();
    assert!(!is_grpc_request(&req));
}
