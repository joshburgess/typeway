use typeway_grpc::web::{
    encode_trailers_frame, is_grpc_web_request, GrpcWebLayer, GrpcWebService, TRAILERS_FRAME_FLAG,
};

#[test]
fn is_grpc_web_request_detects_binary() {
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "application/grpc-web")
        .body(())
        .unwrap();
    assert!(is_grpc_web_request(&req));
}

#[test]
fn is_grpc_web_request_detects_json() {
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "application/grpc-web+json")
        .body(())
        .unwrap();
    assert!(is_grpc_web_request(&req));
}

#[test]
fn is_grpc_web_request_rejects_standard_grpc() {
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "application/grpc")
        .body(())
        .unwrap();
    assert!(!is_grpc_web_request(&req));
}

#[test]
fn is_grpc_web_request_rejects_json() {
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(())
        .unwrap();
    assert!(!is_grpc_web_request(&req));
}

#[test]
fn is_grpc_web_request_rejects_text_html() {
    let req = http::Request::builder()
        .header(http::header::CONTENT_TYPE, "text/html")
        .body(())
        .unwrap();
    assert!(!is_grpc_web_request(&req));
}

#[test]
fn is_grpc_web_request_rejects_no_content_type() {
    let req = http::Request::builder().body(()).unwrap();
    assert!(!is_grpc_web_request(&req));
}

#[test]
fn grpc_web_layer_constructs() {
    let _layer = GrpcWebLayer::new();
    let _default = GrpcWebLayer;
}

#[test]
fn grpc_web_layer_produces_service() {
    #[derive(Clone)]
    struct FakeInner;

    let layer = GrpcWebLayer::new();
    let _svc: GrpcWebService<FakeInner> = tower_layer::Layer::layer(&layer, FakeInner);
}

#[test]
fn trailers_frame_encoding_flag_byte() {
    let frame = encode_trailers_frame("0", None);
    assert_eq!(frame[0], TRAILERS_FRAME_FLAG);
    assert_eq!(frame[0], 0x80);
}

#[test]
fn trailers_frame_encoding_length_prefix() {
    let frame = encode_trailers_frame("0", None);
    let declared_len = u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]) as usize;
    // The frame should be exactly 5 (header) + declared_len bytes.
    assert_eq!(frame.len(), 5 + declared_len);
}

#[test]
fn trailers_frame_encoding_status_only() {
    let frame = encode_trailers_frame("0", None);
    let trailer_text = std::str::from_utf8(&frame[5..]).unwrap();
    assert_eq!(trailer_text, "grpc-status: 0\r\n");
}

#[test]
fn trailers_frame_encoding_status_and_message() {
    let frame = encode_trailers_frame("13", Some("something went wrong"));
    let trailer_text = std::str::from_utf8(&frame[5..]).unwrap();
    assert!(trailer_text.contains("grpc-status: 13\r\n"));
    assert!(trailer_text.contains("grpc-message: something went wrong\r\n"));
}

#[test]
fn trailers_frame_encoding_various_codes() {
    for code in ["0", "1", "2", "5", "12", "13", "14", "16"] {
        let frame = encode_trailers_frame(code, None);
        assert_eq!(frame[0], 0x80);
        let trailer_text = std::str::from_utf8(&frame[5..]).unwrap();
        assert!(
            trailer_text.contains(&format!("grpc-status: {}", code)),
            "expected grpc-status: {} in trailer text: {}",
            code,
            trailer_text
        );
    }
}
