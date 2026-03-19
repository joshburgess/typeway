//! Streaming response support for the [`Client`](crate::Client).
//!
//! The [`call_streaming`](crate::Client::call_streaming) method sends a
//! request but returns the raw [`reqwest::Response`] instead of
//! deserializing the body. This is useful for SSE streams, file downloads,
//! or any large response where buffering the entire body is undesirable.

use crate::call::CallEndpoint;
use crate::client::Client;
use crate::error::ClientError;

impl Client {
    /// Call an endpoint and return the raw response for streaming.
    ///
    /// Unlike [`call`](Client::call), this does not deserialize the response
    /// body. The caller can stream the body using
    /// [`bytes_stream()`](reqwest::Response::bytes_stream) or read it
    /// manually.
    ///
    /// The request is built identically to `call` (path substitution, method,
    /// body serialization, interceptors), but the response is returned as-is
    /// after a status check. A non-2xx status code produces
    /// [`ClientError::Status`].
    ///
    /// Retries are **not** applied — streaming responses are not idempotent
    /// in general and partial reads cannot be rewound.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use futures::StreamExt;
    ///
    /// let resp = client.call_streaming::<GetEndpoint<EventsPath, ()>>(()).await?;
    /// let mut stream = resp.bytes_stream();
    /// while let Some(chunk) = stream.next().await {
    ///     let bytes = chunk?;
    ///     // process chunk...
    /// }
    /// ```
    pub async fn call_streaming<E: CallEndpoint>(
        &self,
        args: E::Args,
    ) -> Result<reqwest::Response, ClientError> {
        let path = E::build_path(&args);
        let url = self.base_url.join(&path)?;
        let method = E::method();

        let mut request = self.inner.request(method, url);

        if let Some(body_result) = E::request_body(&args) {
            let body = body_result?;
            request = request
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(body);
        }

        // Apply request interceptors.
        for interceptor in &self.config.request_interceptors {
            request = interceptor(request);
        }

        let response: reqwest::Response = match request.send().await {
            Ok(resp) => resp,
            Err(e) if e.is_timeout() => return Err(ClientError::Timeout),
            Err(e) => return Err(ClientError::Request(e)),
        };

        // Apply response interceptors.
        for interceptor in &self.config.response_interceptors {
            interceptor(&response);
        }

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ClientError::Status { status, body });
        }

        Ok(response)
    }
}
