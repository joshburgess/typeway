//! A test helper client for making gRPC+json requests in integration tests.
//!
//! [`GrpcTestClient`] handles gRPC framing automatically and provides
//! ergonomic assertion methods on [`GrpcTestResponse`]. It uses
//! `application/grpc+json` encoding — the same format used by the typeway
//! gRPC bridge.
//!
//! # Example
//!
//! ```ignore
//! use typeway_grpc::test_client::GrpcTestClient;
//!
//! let client = GrpcTestClient::new("http://127.0.0.1:3000");
//!
//! let resp = client
//!     .call("users.v1.UserService", "ListUser", serde_json::json!({}))
//!     .await;
//! assert!(resp.is_ok());
//!
//! let users = resp.json();
//! assert!(users.is_array());
//! ```

use crate::framing;
use crate::status::GrpcCode;

/// A test client for making gRPC+json requests with proper framing.
///
/// Encodes request bodies with gRPC length-prefix framing and decodes
/// framed responses automatically. Intended for integration tests, not
/// production use.
pub struct GrpcTestClient {
    base_url: String,
    inner: reqwest::Client,
}

impl GrpcTestClient {
    /// Create a new test client pointing at the given base URL.
    ///
    /// The URL should include the scheme and port, e.g.
    /// `"http://127.0.0.1:3000"`.
    pub fn new(base_url: &str) -> Self {
        GrpcTestClient {
            base_url: base_url.trim_end_matches('/').to_string(),
            inner: reqwest::Client::new(),
        }
    }

    /// Call a gRPC method with a JSON-encoded request body.
    ///
    /// The `service` should be the fully-qualified service path
    /// (e.g., `"users.v1.UserService"`), and `method` is the RPC method
    /// name (e.g., `"ListUser"`).
    ///
    /// The request body is serialized to JSON, then wrapped in a gRPC
    /// length-prefix frame before sending.
    pub async fn call(
        &self,
        service: &str,
        method: &str,
        body: serde_json::Value,
    ) -> GrpcTestResponse {
        let url = format!("{}/{}/{}", self.base_url, service, method);
        let json_bytes = serde_json::to_vec(&body).unwrap_or_default();
        let framed = framing::encode_grpc_frame(&json_bytes);

        let response = self
            .inner
            .post(&url)
            .header("content-type", "application/grpc+json")
            .header("te", "trailers")
            .body(framed)
            .send()
            .await
            .expect("gRPC test request failed");

        let grpc_status = response
            .headers()
            .get("grpc-status")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0i32);

        let grpc_message = response
            .headers()
            .get("grpc-message")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body_bytes = response.bytes().await.unwrap_or_default();

        // Try to decode gRPC frame; fall back to raw bytes if unframed.
        let unframed = framing::decode_grpc_frame(&body_bytes)
            .unwrap_or(&body_bytes);

        let json: serde_json::Value =
            serde_json::from_slice(unframed).unwrap_or(serde_json::Value::Null);

        GrpcTestResponse {
            grpc_status,
            grpc_message,
            body: json,
        }
    }

    /// Call a gRPC method with an empty JSON object as the request body.
    pub async fn call_empty(&self, service: &str, method: &str) -> GrpcTestResponse {
        self.call(service, method, serde_json::json!({})).await
    }
}

/// The response from a [`GrpcTestClient`] call.
///
/// Provides access to the gRPC status code, message, and the decoded
/// JSON response body.
#[derive(Debug)]
pub struct GrpcTestResponse {
    /// The gRPC status code (0 = OK).
    pub grpc_status: i32,
    /// The gRPC error message (empty on success).
    pub grpc_message: String,
    /// The decoded JSON response body.
    pub body: serde_json::Value,
}

impl GrpcTestResponse {
    /// Returns `true` if the gRPC status is 0 (OK).
    pub fn is_ok(&self) -> bool {
        self.grpc_status == 0
    }

    /// Return the gRPC status as a [`GrpcCode`] enum value.
    pub fn grpc_code(&self) -> GrpcCode {
        GrpcCode::from_i32(self.grpc_status)
    }

    /// Return a reference to the decoded JSON response body.
    pub fn json(&self) -> &serde_json::Value {
        &self.body
    }
}
