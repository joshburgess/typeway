//! gRPC length-prefix framing.
//!
//! gRPC messages are framed with a 5-byte header:
//! - 1 byte: compression flag (0 = not compressed)
//! - 4 bytes: big-endian message length
//!
//! This module provides [`encode_grpc_frame`] and [`decode_grpc_frame`] for
//! encoding and decoding gRPC-framed messages, regardless of payload format
//! (JSON or protobuf).

/// Encode a message with gRPC length-prefix framing.
///
/// Format: `[1 byte: compressed flag (0)] [4 bytes: big-endian length] [message bytes]`
///
/// # Example
///
/// ```
/// use typeway_grpc::framing::encode_grpc_frame;
///
/// let msg = b"hello";
/// let frame = encode_grpc_frame(msg);
/// assert_eq!(frame.len(), 5 + msg.len());
/// assert_eq!(frame[0], 0); // not compressed
/// ```
pub fn encode_grpc_frame(message: &[u8]) -> Vec<u8> {
    let len = message.len() as u32;
    let mut frame = Vec::with_capacity(5 + message.len());
    frame.push(0); // not compressed
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(message);
    frame
}

/// Decode a gRPC length-prefix framed message.
///
/// Returns the message bytes without the frame header. If the input is
/// shorter than 5 bytes or the declared length exceeds the available data,
/// an error is returned.
///
/// # Example
///
/// ```
/// use typeway_grpc::framing::{encode_grpc_frame, decode_grpc_frame};
///
/// let original = b"hello";
/// let frame = encode_grpc_frame(original);
/// let decoded = decode_grpc_frame(&frame).unwrap();
/// assert_eq!(decoded, original);
/// ```
pub fn decode_grpc_frame(data: &[u8]) -> Result<&[u8], FramingError> {
    if data.len() < 5 {
        return Err(FramingError::TooShort {
            got: data.len(),
        });
    }
    let _compressed = data[0]; // TODO: handle compression
    let len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
    if data.len() < 5 + len {
        return Err(FramingError::Incomplete {
            expected: len,
            got: data.len() - 5,
        });
    }
    Ok(&data[5..5 + len])
}

/// Errors that can occur when decoding a gRPC frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FramingError {
    /// The input is too short to contain a gRPC frame header (need at least 5 bytes).
    TooShort {
        /// Number of bytes actually provided.
        got: usize,
    },
    /// The frame header declares a message length that exceeds the available data.
    Incomplete {
        /// The declared message length from the header.
        expected: usize,
        /// The number of bytes actually available after the header.
        got: usize,
    },
}

impl std::fmt::Display for FramingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FramingError::TooShort { got } => {
                write!(f, "frame too short: got {got} bytes, need at least 5")
            }
            FramingError::Incomplete { expected, got } => {
                write!(
                    f,
                    "incomplete frame: expected {expected} bytes, got {got}"
                )
            }
        }
    }
}

impl std::error::Error for FramingError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let msg = b"hello, world";
        let frame = encode_grpc_frame(msg);
        let decoded = decode_grpc_frame(&frame).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn empty_message() {
        let msg = b"";
        let frame = encode_grpc_frame(msg);
        assert_eq!(frame.len(), 5);
        let decoded = decode_grpc_frame(&frame).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn large_message() {
        let msg = vec![0xABu8; 100_000];
        let frame = encode_grpc_frame(&msg);
        assert_eq!(frame.len(), 5 + 100_000);
        let decoded = decode_grpc_frame(&frame).unwrap();
        assert_eq!(decoded, &msg[..]);
    }

    #[test]
    fn frame_header_format() {
        let msg = b"test";
        let frame = encode_grpc_frame(msg);
        // Byte 0: compression flag = 0
        assert_eq!(frame[0], 0);
        // Bytes 1-4: big-endian length = 4
        assert_eq!(&frame[1..5], &[0, 0, 0, 4]);
        // Bytes 5+: the message
        assert_eq!(&frame[5..], b"test");
    }

    #[test]
    fn decode_too_short() {
        assert_eq!(
            decode_grpc_frame(&[]),
            Err(FramingError::TooShort { got: 0 })
        );
        assert_eq!(
            decode_grpc_frame(&[0, 0, 0]),
            Err(FramingError::TooShort { got: 3 })
        );
        assert_eq!(
            decode_grpc_frame(&[0, 0, 0, 0]),
            Err(FramingError::TooShort { got: 4 })
        );
    }

    #[test]
    fn decode_incomplete() {
        // Header says 10 bytes, but only 3 available after header.
        let data = [0, 0, 0, 0, 10, 1, 2, 3];
        assert_eq!(
            decode_grpc_frame(&data),
            Err(FramingError::Incomplete {
                expected: 10,
                got: 3,
            })
        );
    }

    #[test]
    fn decode_exact_length() {
        // Header says 3 bytes, exactly 3 available.
        let data = [0, 0, 0, 0, 3, b'a', b'b', b'c'];
        let decoded = decode_grpc_frame(&data).unwrap();
        assert_eq!(decoded, b"abc");
    }

    #[test]
    fn decode_with_extra_trailing_bytes() {
        // Header says 2 bytes, but 5 available after header.
        // Should only return the first 2.
        let data = [0, 0, 0, 0, 2, b'a', b'b', b'c', b'd', b'e'];
        let decoded = decode_grpc_frame(&data).unwrap();
        assert_eq!(decoded, b"ab");
    }

    #[test]
    fn error_display() {
        let err = FramingError::TooShort { got: 2 };
        assert_eq!(
            err.to_string(),
            "frame too short: got 2 bytes, need at least 5"
        );

        let err = FramingError::Incomplete {
            expected: 10,
            got: 3,
        };
        assert_eq!(
            err.to_string(),
            "incomplete frame: expected 10 bytes, got 3"
        );
    }
}
