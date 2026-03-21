//! gRPC compression negotiation and encoding.
//!
//! Supports `gzip` and `deflate` compression as specified by the gRPC protocol.
//! Compression is negotiated via the `grpc-encoding` (incoming) and
//! `grpc-accept-encoding` (response negotiation) headers.

use std::io::{Read, Write};

/// Supported compression algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// Gzip compression.
    Gzip,
    /// Deflate (zlib) compression.
    Deflate,
}

impl Compression {
    /// The header value for this compression algorithm.
    pub fn as_str(&self) -> &'static str {
        match self {
            Compression::Gzip => "gzip",
            Compression::Deflate => "deflate",
        }
    }
}

/// Parse the `grpc-encoding` header to determine incoming message compression.
///
/// Returns `None` if the header is absent, `"identity"`, or unrecognized.
pub fn incoming_compression(headers: &http::HeaderMap) -> Option<Compression> {
    let encoding = headers.get("grpc-encoding")?.to_str().ok()?;
    match encoding {
        "gzip" => Some(Compression::Gzip),
        "deflate" => Some(Compression::Deflate),
        _ => None,
    }
}

/// Parse the `grpc-accept-encoding` header to negotiate response compression.
///
/// Returns the first supported algorithm found in the client's preference list.
pub fn negotiate_compression(headers: &http::HeaderMap) -> Option<Compression> {
    let accept = headers.get("grpc-accept-encoding")?.to_str().ok()?;
    // Algorithms listed in preference order.
    for part in accept.split(',') {
        match part.trim() {
            "gzip" => return Some(Compression::Gzip),
            "deflate" => return Some(Compression::Deflate),
            _ => continue,
        }
    }
    None
}

/// Compress a byte slice with the given algorithm.
pub fn compress(data: &[u8], algo: Compression) -> Result<Vec<u8>, CompressionError> {
    match algo {
        Compression::Gzip => {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(data)?;
            Ok(encoder.finish()?)
        }
        Compression::Deflate => {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(data)?;
            Ok(encoder.finish()?)
        }
    }
}

/// Decompress a byte slice with the given algorithm.
pub fn decompress(data: &[u8], algo: Compression) -> Result<Vec<u8>, CompressionError> {
    match algo {
        Compression::Gzip => {
            let mut decoder = flate2::read::GzDecoder::new(data);
            let mut buf = Vec::new();
            decoder.read_to_end(&mut buf)?;
            Ok(buf)
        }
        Compression::Deflate => {
            let mut decoder = flate2::read::ZlibDecoder::new(data);
            let mut buf = Vec::new();
            decoder.read_to_end(&mut buf)?;
            Ok(buf)
        }
    }
}

/// Encode a gRPC frame with compression.
///
/// Sets the compression flag to `1` in the frame header.
/// Returns the 5-byte header + compressed payload.
pub fn encode_compressed_frame(
    message: &[u8],
    algo: Compression,
) -> Result<Vec<u8>, CompressionError> {
    let compressed = compress(message, algo)?;
    let len = compressed.len() as u32;
    let mut frame = Vec::with_capacity(5 + compressed.len());
    frame.push(1); // compressed flag
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(&compressed);
    Ok(frame)
}

/// Decode a gRPC frame, handling decompression if the compression flag is set.
///
/// Returns the decompressed payload.
pub fn decode_frame_with_decompression(
    data: &[u8],
    compression: Option<Compression>,
) -> Result<Vec<u8>, CompressionError> {
    if data.len() < 5 {
        return Err(CompressionError::FrameTooShort);
    }

    let compressed_flag = data[0];
    let len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;

    if data.len() < 5 + len {
        return Err(CompressionError::FrameIncomplete);
    }

    let payload = &data[5..5 + len];

    if compressed_flag == 1 {
        match compression {
            Some(algo) => decompress(payload, algo),
            None => Err(CompressionError::CompressedButNoAlgorithm),
        }
    } else {
        Ok(payload.to_vec())
    }
}

/// Error from compression or decompression.
#[derive(Debug)]
pub enum CompressionError {
    /// I/O error during compression/decompression.
    Io(std::io::Error),
    /// Frame is too short to contain a header.
    FrameTooShort,
    /// Frame declares a length exceeding available data.
    FrameIncomplete,
    /// Frame has compression flag set but no algorithm was negotiated.
    CompressedButNoAlgorithm,
}

impl From<std::io::Error> for CompressionError {
    fn from(e: std::io::Error) -> Self {
        CompressionError::Io(e)
    }
}

impl std::fmt::Display for CompressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressionError::Io(e) => write!(f, "compression I/O error: {e}"),
            CompressionError::FrameTooShort => write!(f, "gRPC frame too short"),
            CompressionError::FrameIncomplete => write!(f, "gRPC frame incomplete"),
            CompressionError::CompressedButNoAlgorithm => {
                write!(f, "compressed frame but no algorithm negotiated")
            }
        }
    }
}

impl std::error::Error for CompressionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_roundtrip() {
        let data = b"hello, gRPC compression!";
        let compressed = compress(data, Compression::Gzip).unwrap();
        let decompressed = decompress(&compressed, Compression::Gzip).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn deflate_roundtrip() {
        let data = b"hello, deflate!";
        let compressed = compress(data, Compression::Deflate).unwrap();
        let decompressed = decompress(&compressed, Compression::Deflate).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn compressed_frame_roundtrip() {
        let message = b"test message";
        let frame = encode_compressed_frame(message, Compression::Gzip).unwrap();
        assert_eq!(frame[0], 1); // compression flag
        let decoded =
            decode_frame_with_decompression(&frame, Some(Compression::Gzip)).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn uncompressed_frame_passthrough() {
        let message = b"uncompressed";
        let mut frame = vec![0u8]; // no compression
        let len = message.len() as u32;
        frame.extend_from_slice(&len.to_be_bytes());
        frame.extend_from_slice(message);

        let decoded = decode_frame_with_decompression(&frame, None).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn negotiate_gzip() {
        let mut headers = http::HeaderMap::new();
        headers.insert("grpc-accept-encoding", "gzip, deflate, identity".parse().unwrap());
        assert_eq!(negotiate_compression(&headers), Some(Compression::Gzip));
    }

    #[test]
    fn negotiate_deflate_only() {
        let mut headers = http::HeaderMap::new();
        headers.insert("grpc-accept-encoding", "deflate".parse().unwrap());
        assert_eq!(negotiate_compression(&headers), Some(Compression::Deflate));
    }

    #[test]
    fn negotiate_none() {
        let headers = http::HeaderMap::new();
        assert_eq!(negotiate_compression(&headers), None);
    }

    #[test]
    fn incoming_gzip() {
        let mut headers = http::HeaderMap::new();
        headers.insert("grpc-encoding", "gzip".parse().unwrap());
        assert_eq!(incoming_compression(&headers), Some(Compression::Gzip));
    }

    #[test]
    fn incoming_identity() {
        let mut headers = http::HeaderMap::new();
        headers.insert("grpc-encoding", "identity".parse().unwrap());
        assert_eq!(incoming_compression(&headers), None);
    }
}
