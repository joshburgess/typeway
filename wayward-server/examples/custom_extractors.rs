//! Demonstrates implementing custom extractors.
//!
//! Run: cargo run -p wayward-server --example custom_extractors
//! Test:
//!   curl http://127.0.0.1:3000/hello                          # 401
//!   curl -H "Authorization: Bearer secret" http://127.0.0.1:3000/hello  # 200
//!   curl -X POST http://127.0.0.1:3000/echo -d '{"msg":"hi"}'  # echoes

use wayward_core::*;
use wayward_macros::*;
use wayward_server::*;

// ---------------------------------------------------------------------------
// Custom extractor #1: BearerToken (FromRequestParts)
//
// Extracts a bearer token from the Authorization header.
// Implement FromRequestParts for metadata extractors (headers, query, etc.)
// ---------------------------------------------------------------------------

struct BearerToken(String);

impl FromRequestParts for BearerToken {
    type Error = JsonError;

    fn from_request_parts(parts: &http::request::Parts) -> Result<Self, Self::Error> {
        let header = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .ok_or_else(|| JsonError::unauthorized("missing Authorization header"))?
            .to_str()
            .map_err(|_| JsonError::bad_request("invalid Authorization header"))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| JsonError::unauthorized("expected Bearer token"))?;

        Ok(BearerToken(token.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Custom extractor #2: Xml<T> (FromRequest)
//
// Shows how to implement a body extractor for a custom content type.
// Implement FromRequest for body extractors (last handler arg only).
// ---------------------------------------------------------------------------

struct RawBody(String);

impl FromRequest for RawBody {
    type Error = (http::StatusCode, String);

    async fn from_request(
        _parts: &http::request::Parts,
        body: bytes::Bytes,
    ) -> Result<Self, Self::Error> {
        String::from_utf8(body.to_vec()).map(RawBody).map_err(|e| {
            (
                http::StatusCode::BAD_REQUEST,
                format!("invalid UTF-8 body: {e}"),
            )
        })
    }
}

// ---------------------------------------------------------------------------
// Handlers using custom extractors
// ---------------------------------------------------------------------------

wayward_path!(type HelloPath = "hello");
wayward_path!(type EchoPath = "echo");

type API = (
    GetEndpoint<HelloPath, String>,
    PostEndpoint<EchoPath, String, String>,
);

async fn hello(token: BearerToken) -> String {
    format!("Authenticated with token: {}", token.0)
}

async fn echo(_token: BearerToken, body: RawBody) -> String {
    format!("Echo: {}", body.0)
}

#[tokio::main]
async fn main() {
    let server = Server::<API>::new((bind::<_, _, _>(hello), bind::<_, _, _>(echo)));

    println!("Custom extractors example on http://127.0.0.1:3000");
    println!("  GET  /hello - requires Bearer token");
    println!("  POST /echo  - requires Bearer token, echoes body");

    server
        .serve("127.0.0.1:3000".parse().unwrap())
        .await
        .unwrap();
}
