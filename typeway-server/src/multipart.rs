//! Multipart form data extraction.
//!
//! Enabled with `feature = "multipart"`. Provides [`Multipart`] for
//! parsing `multipart/form-data` requests (file uploads).
//!
//! # Example
//!
//! ```ignore
//! use typeway_server::multipart::Multipart;
//!
//! async fn upload(multipart: Multipart) -> Result<String, JsonError> {
//!     let mut files = Vec::new();
//!     while let Some(field) = multipart.next_field().await? {
//!         let name = field.name().unwrap_or("unknown").to_string();
//!         let data = field.bytes().await?;
//!         files.push(format!("{name}: {} bytes", data.len()));
//!     }
//!     Ok(files.join(", "))
//! }
//! ```

use crate::extract::FromRequest;
use http::StatusCode;

/// Multipart form data extractor.
///
/// Wraps `multer::Multipart` for parsing `multipart/form-data` bodies.
/// Use as the last handler argument (body extractor).
pub struct Multipart(multer::Multipart<'static>);

impl Multipart {
    /// Get the next field in the multipart stream.
    pub async fn next_field(&mut self) -> Result<Option<multer::Field<'static>>, multer::Error> {
        self.0.next_field().await
    }
}

impl FromRequest for Multipart {
    type Error = (StatusCode, String);

    async fn from_request(
        parts: &http::request::Parts,
        body: bytes::Bytes,
    ) -> Result<Self, Self::Error> {
        let boundary = parts
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|ct| ct.to_str().ok())
            .and_then(|ct| multer::parse_boundary(ct).ok())
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "missing or invalid multipart boundary".to_string(),
                )
            })?;

        let stream = futures::stream::once(async move { Ok::<_, std::io::Error>(body) });
        let multipart = multer::Multipart::new(stream, boundary);
        Ok(Multipart(multipart))
    }
}
