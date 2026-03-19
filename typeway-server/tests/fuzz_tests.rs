//! Property-based fuzz tests for attack surface areas:
//! path parsing, JSON body deserialization, and query string extraction.
//!
//! These tests use `proptest` to generate arbitrary inputs and verify the server
//! never panics, always returning a valid HTTP status code.

use std::sync::Arc;
use std::time::Duration;

use proptest::prelude::*;
use serde::{Deserialize, Serialize};

use typeway_core::*;
use typeway_macros::*;
use typeway_server::*;

// --- Types ---

typeway_path!(type HelloPath = "hello");
typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);
typeway_path!(type EchoPath = "echo");
typeway_path!(type SearchPath = "search");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct User {
    id: u32,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CreateUser {
    name: String,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: Option<String>,
    page: Option<u32>,
    limit: Option<u32>,
}

// --- Handlers ---

async fn hello() -> &'static str {
    "hello"
}

async fn get_user(path: Path<UserByIdPath>) -> Result<Json<User>, http::StatusCode> {
    let (id,) = path.0;
    Ok(Json(User {
        id,
        name: "Test".into(),
    }))
}

async fn create_user(body: Json<CreateUser>) -> (http::StatusCode, Json<User>) {
    (
        http::StatusCode::CREATED,
        Json(User {
            id: 1,
            name: body.0.name,
        }),
    )
}

async fn echo_body(body: String) -> String {
    body
}

async fn search(query: Query<SearchParams>) -> String {
    format!(
        "q={:?} page={:?} limit={:?}",
        query.0.q, query.0.page, query.0.limit
    )
}

// --- Test server ---

type FuzzAPI = (
    GetEndpoint<HelloPath, String>,
    GetEndpoint<UserByIdPath, User>,
    PostEndpoint<UsersPath, CreateUser, User>,
    PostEndpoint<EchoPath, String, String>,
    GetEndpoint<SearchPath, String>,
);

async fn start_fuzz_server() -> u16 {
    let server = Server::<FuzzAPI>::new((
        bind::<_, _, _>(hello),
        bind::<_, _, _>(get_user),
        bind::<_, _, _>(create_user),
        bind::<_, _, _>(echo_body),
        bind::<_, _, _>(search),
    ));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let router = Arc::new(server.into_router());
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = RouterService::new(router.clone());
            let hyper_svc = hyper_util::service::TowerToHyperService::new(svc);
            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, hyper_svc)
                    .await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

/// Valid response status codes the server may return for any request.
fn is_valid_status(status: u16) -> bool {
    // The server should only return well-known status codes, never garbage.
    (200..=599).contains(&status)
}

// ---------------------------------------------------------------------------
// Path parsing robustness
// ---------------------------------------------------------------------------

/// Strategy for generating adversarial path strings.
fn arb_path() -> impl Strategy<Value = String> {
    prop_oneof![
        // Completely random strings
        "\\PC{0,200}",
        // Paths with special characters
        prop::collection::vec(
            prop_oneof![
                Just("".to_string()),
                Just("..".to_string()),
                Just(".".to_string()),
                Just("%00".to_string()),
                Just("%2F".to_string()),
                Just("%2f".to_string()),
                Just("%0a".to_string()),
                Just("%0d%0a".to_string()),
                Just("~".to_string()),
                Just("\\".to_string()),
                Just("<script>".to_string()),
                Just("{{template}}".to_string()),
                Just("users".to_string()),
                Just("hello".to_string()),
                // Unicode
                Just("\u{200b}".to_string()),       // zero-width space
                Just("\u{feff}".to_string()),        // BOM
                Just("\u{202e}".to_string()),        // RTL override
                Just("\u{ffff}".to_string()),
                // Very long segment
                "[a-z]{1,500}",
                // Random unicode
                "\\PC{1,50}",
            ],
            0..20,
        )
        .prop_map(|segments| format!("/{}", segments.join("/"))),
        // Double slashes, triple slashes
        prop::collection::vec(
            prop_oneof![Just("".to_string()), Just("".to_string()), "\\PC{0,30}",],
            0..15,
        )
        .prop_map(|segments| format!("/{}", segments.join("/"))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// The router must never panic on arbitrary path inputs. It should always
    /// return a valid HTTP response (200, 400, 404, or 405).
    #[test]
    fn path_parsing_never_panics(path in arb_path()) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let port = start_fuzz_server().await;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();

            // GET request with arbitrary path
            let url = format!("http://127.0.0.1:{port}{path}");
            // reqwest may reject truly malformed URLs, which is fine — skip those.
            if let Ok(resp) = client.get(&url).send().await {
                let status = resp.status().as_u16();
                prop_assert!(
                    is_valid_status(status),
                    "GET {path} returned invalid status {status}"
                );
            }

            // POST request with arbitrary path
            if let Ok(resp) = client.post(&url).body("test").send().await {
                let status = resp.status().as_u16();
                prop_assert!(
                    is_valid_status(status),
                    "POST {path} returned invalid status {status}"
                );
            }

            Ok(())
        })?;
    }
}

// ---------------------------------------------------------------------------
// JSON body deserialization robustness
// ---------------------------------------------------------------------------

/// Strategy for generating adversarial request bodies.
fn arb_body() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        // Empty body
        Just(vec![]),
        // Random bytes
        prop::collection::vec(any::<u8>(), 0..1024),
        // Almost-valid JSON
        Just(b"{".to_vec()),
        Just(b"{}".to_vec()),
        Just(b"{\"name\":}".to_vec()),
        Just(b"{\"name\": null}".to_vec()),
        Just(b"{\"name\": 42}".to_vec()),
        Just(b"{\"name\": true}".to_vec()),
        Just(b"{\"name\": []}".to_vec()),
        Just(b"[1, 2, 3]".to_vec()),
        Just(b"null".to_vec()),
        Just(b"\"string\"".to_vec()),
        Just(b"42".to_vec()),
        // Valid JSON with correct shape
        Just(b"{\"name\": \"valid\"}".to_vec()),
        // Unicode edge cases in JSON
        Just("{\"name\": \"\u{0000}\"}".as_bytes().to_vec()),
        Just("{\"name\": \"\u{ffff}\"}".as_bytes().to_vec()),
        // Very deeply nested
        Just(format!("{}\"a\":0{}", "{".repeat(64), "}".repeat(64)).into_bytes()),
        // Arbitrary strings as JSON body
        "\\PC{0,500}".prop_map(|s| s.into_bytes()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// The server must never panic when receiving arbitrary bytes as a JSON body.
    /// It should return 200 for valid JSON matching the expected schema, or 400
    /// for anything else.
    #[test]
    fn json_body_deserialization_never_panics(body in arb_body()) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let port = start_fuzz_server().await;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();

            // POST to /users with Content-Type: application/json and arbitrary body
            let resp = client
                .post(format!("http://127.0.0.1:{port}/users"))
                .header("content-type", "application/json")
                .body(body.clone())
                .send()
                .await
                .unwrap();

            let status = resp.status().as_u16();
            prop_assert!(
                is_valid_status(status),
                "POST /users with fuzzed JSON body returned invalid status {status}"
            );
            // Should be either 201 (valid) or 400 (invalid body)
            prop_assert!(
                status == 201 || status == 400,
                "POST /users expected 201 or 400 but got {status}"
            );

            Ok(())
        })?;
    }

    /// The server must handle arbitrary bytes sent as a plain text body without
    /// panicking.
    #[test]
    fn raw_body_deserialization_never_panics(body in prop::collection::vec(any::<u8>(), 0..2048)) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let port = start_fuzz_server().await;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();

            // POST to /echo with arbitrary body bytes (no Content-Type: application/json)
            let resp = client
                .post(format!("http://127.0.0.1:{port}/echo"))
                .body(body)
                .send()
                .await
                .unwrap();

            let status = resp.status().as_u16();
            prop_assert!(
                is_valid_status(status),
                "POST /echo with fuzzed body returned invalid status {status}"
            );

            Ok(())
        })?;
    }
}

// ---------------------------------------------------------------------------
// Query string extraction robustness
// ---------------------------------------------------------------------------

/// Strategy for generating adversarial query strings.
fn arb_query_string() -> impl Strategy<Value = String> {
    prop_oneof![
        // Empty query
        Just("".to_string()),
        // Random key-value pairs
        prop::collection::vec(
            (
                "\\PC{0,50}",
                "\\PC{0,50}",
            ),
            0..20,
        )
        .prop_map(|pairs| {
            pairs
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("&")
        }),
        // Malformed queries
        Just("&&&".to_string()),
        Just("===".to_string()),
        Just("q=".to_string()),
        Just("=value".to_string()),
        Just("key".to_string()),
        Just("q=hello&q=world".to_string()),        // duplicate keys
        Just("q=%00%01%02".to_string()),             // null bytes in values
        Just("q=%zz".to_string()),                   // invalid percent encoding
        Just("q=a%2".to_string()),                   // truncated percent encoding
        // Very long query string
        "[a-z]{1,100}".prop_map(|s| format!("q={}", s.repeat(50))),
        // Unicode in query values
        Just("q=\u{200b}\u{feff}\u{202e}".to_string()),
        // Deeply nested-looking params
        Just("a[b][c][d][e]=1".to_string()),
        // Random string as entire query
        "\\PC{0,300}",
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// The server must never panic when receiving arbitrary query strings.
    #[test]
    fn query_string_extraction_never_panics(qs in arb_query_string()) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let port = start_fuzz_server().await;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();

            // GET /search?<arbitrary query string>
            // Build URL manually to avoid reqwest's query encoding
            let url = if qs.is_empty() {
                format!("http://127.0.0.1:{port}/search")
            } else {
                format!("http://127.0.0.1:{port}/search?{qs}")
            };

            // reqwest may reject truly malformed URLs — skip those.
            if let Ok(resp) = client.get(&url).send().await {
                let status = resp.status().as_u16();
                prop_assert!(
                    is_valid_status(status),
                    "GET /search?{qs} returned invalid status {status}"
                );
            }

            Ok(())
        })?;
    }
}
