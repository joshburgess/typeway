//! Tests for gRPC length-prefix framing.

use typeway_grpc::framing::{decode_grpc_frame, encode_grpc_frame, FramingError};

#[test]
fn roundtrip_returns_original_bytes() {
    let msg = b"hello, gRPC world";
    let frame = encode_grpc_frame(msg);
    let decoded = decode_grpc_frame(&frame).unwrap();
    assert_eq!(decoded, msg);
}

#[test]
fn empty_message_roundtrip() {
    let msg = b"";
    let frame = encode_grpc_frame(msg);
    assert_eq!(frame.len(), 5);
    let decoded = decode_grpc_frame(&frame).unwrap();
    assert!(decoded.is_empty());
}

#[test]
fn large_message_roundtrip() {
    let msg = vec![0xFFu8; 100_000];
    let frame = encode_grpc_frame(&msg);
    assert_eq!(frame.len(), 5 + 100_000);
    let decoded = decode_grpc_frame(&frame).unwrap();
    assert_eq!(decoded, &msg[..]);
}

#[test]
fn frame_header_format_is_correct() {
    let msg = b"test";
    let frame = encode_grpc_frame(msg);

    // Byte 0: compression flag = 0
    assert_eq!(frame[0], 0);
    // Bytes 1-4: big-endian length = 4
    assert_eq!(frame[1], 0);
    assert_eq!(frame[2], 0);
    assert_eq!(frame[3], 0);
    assert_eq!(frame[4], 4);
    // Bytes 5+: message payload
    assert_eq!(&frame[5..], b"test");
}

#[test]
fn decode_too_short_input_returns_error() {
    // Empty input
    let err = decode_grpc_frame(&[]).unwrap_err();
    assert!(matches!(err, FramingError::TooShort { got: 0 }));

    // 3 bytes (need at least 5)
    let err = decode_grpc_frame(&[0, 0, 0]).unwrap_err();
    assert!(matches!(err, FramingError::TooShort { got: 3 }));

    // 4 bytes (still too short)
    let err = decode_grpc_frame(&[0, 0, 0, 0]).unwrap_err();
    assert!(matches!(err, FramingError::TooShort { got: 4 }));
}

#[test]
fn decode_incomplete_frame_returns_error() {
    // Header says 10 bytes, but only 2 available.
    let data = [0, 0, 0, 0, 10, 0xAA, 0xBB];
    let err = decode_grpc_frame(&data).unwrap_err();
    assert!(matches!(
        err,
        FramingError::Incomplete {
            expected: 10,
            got: 2
        }
    ));
}

#[test]
fn json_payload_roundtrip() {
    let json = br#"{"id":1,"name":"Alice"}"#;
    let frame = encode_grpc_frame(json);
    let decoded = decode_grpc_frame(&frame).unwrap();
    assert_eq!(decoded, json);
}

#[test]
fn frame_with_256_byte_message() {
    // Length = 256 = 0x00000100 in big-endian
    let msg = vec![0x42u8; 256];
    let frame = encode_grpc_frame(&msg);
    assert_eq!(frame[1..5], [0, 0, 1, 0]);
    let decoded = decode_grpc_frame(&frame).unwrap();
    assert_eq!(decoded.len(), 256);
}

#[test]
fn error_implements_display() {
    let err = FramingError::TooShort { got: 2 };
    let msg = err.to_string();
    assert!(msg.contains("2"));
    assert!(msg.contains("5"));

    let err = FramingError::Incomplete {
        expected: 100,
        got: 50,
    };
    let msg = err.to_string();
    assert!(msg.contains("100"));
    assert!(msg.contains("50"));
}

#[test]
fn error_implements_std_error() {
    let err: Box<dyn std::error::Error> = Box::new(FramingError::TooShort { got: 0 });
    let _ = err.to_string(); // should not panic
}
