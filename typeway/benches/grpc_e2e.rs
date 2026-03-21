//! End-to-end gRPC benchmark: typeway-grpc vs Tonic.
//!
//! Measures real request latency through the full stack:
//! HTTP/2 → gRPC frame decode → dispatch → handler → encode → HTTP/2 response.
//!
//! Both servers handle the same API (CreateUser: string → {id, name}).
//! The client uses reqwest HTTP/2 with gRPC+JSON framing for fair comparison
//! (both servers receive and respond with the same wire format).
//!
//! Run with: `cargo bench --bench grpc_e2e --features grpc`

use std::sync::Arc;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use serde::{Deserialize, Serialize};

use typeway_core::endpoint::{GetEndpoint, PostEndpoint};
use typeway_core::path::{HCons, HNil, Lit, LitSegment};
use typeway_grpc::mapping::ToProtoType;
use typeway_server::*;

// ---------------------------------------------------------------------------
// Shared domain types
// ---------------------------------------------------------------------------

#[allow(non_camel_case_types)]
struct __lit_users;
impl LitSegment for __lit_users {
    const VALUE: &'static str = "users";
}

type UsersPath = HCons<Lit<__lit_users>, HNil>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
}

impl ToProtoType for User {
    fn proto_type_name() -> &'static str { "User" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message User {\n  uint32 id = 1;\n  string name = 2;\n}".to_string())
    }
}

#[derive(Debug, Deserialize)]
struct CreateUser {
    name: String,
}

impl ToProtoType for CreateUser {
    fn proto_type_name() -> &'static str { "CreateUser" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message CreateUser {\n  string name = 1;\n}".to_string())
    }
}

// ---------------------------------------------------------------------------
// Typeway server
// ---------------------------------------------------------------------------

type TestAPI = (
    GetEndpoint<UsersPath, Vec<User>>,
    PostEndpoint<UsersPath, CreateUser, User>,
);

async fn tw_list_users() -> Json<Vec<User>> {
    Json(vec![
        User { id: 1, name: "Alice".into() },
        User { id: 2, name: "Bob".into() },
    ])
}

async fn tw_create_user(body: Json<CreateUser>) -> (http::StatusCode, Json<User>) {
    let user = User { id: 3, name: body.0.name };
    (http::StatusCode::CREATED, Json(user))
}

async fn start_typeway_server() -> u16 {
    let server = Server::<TestAPI>::new((
        bind::<_, _, _>(tw_list_users),
        bind::<_, _, _>(tw_create_user),
    ))
    .with_grpc("BenchService", "bench.v1");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        server
            .serve_with_shutdown(listener, std::future::pending())
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

// ---------------------------------------------------------------------------
// Tonic server (manual, no codegen — same handler logic)
// ---------------------------------------------------------------------------

async fn start_tonic_server() -> u16 {
    use std::convert::Infallible;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use bytes::Bytes;
    use http_body_util::{BodyExt, Full};

    #[derive(Clone)]
    struct TonicSvc;

    impl tower_service::Service<http::Request<hyper::body::Incoming>> for TonicSvc {
        type Response = http::Response<Full<Bytes>>;
        type Error = Infallible;
        type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<hyper::body::Incoming>) -> Self::Future {
            Box::pin(async move {
                let path = req.uri().path().to_string();

                // Collect body.
                let body_bytes = req.into_body().collect().await
                    .map(|c| c.to_bytes())
                    .unwrap_or_default();

                // Strip gRPC framing.
                let unframed = if body_bytes.len() >= 5 {
                    &body_bytes[5..]
                } else {
                    &body_bytes[..]
                };

                // Dispatch by path.
                let (response_json, grpc_code) = if path.ends_with("ListUser") {
                    let users = vec![
                        serde_json::json!({"id": 1, "name": "Alice"}),
                        serde_json::json!({"id": 2, "name": "Bob"}),
                    ];
                    (serde_json::to_vec(&users).unwrap(), 0i32)
                } else if path.ends_with("CreateUser") {
                    let input: serde_json::Value = serde_json::from_slice(unframed)
                        .unwrap_or_default();
                    let name = input.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let user = serde_json::json!({"id": 3, "name": name});
                    (serde_json::to_vec(&user).unwrap(), 0i32)
                } else {
                    (Vec::new(), 12) // UNIMPLEMENTED
                };

                // gRPC frame the response.
                let len = response_json.len() as u32;
                let mut framed = Vec::with_capacity(5 + response_json.len());
                framed.push(0); // not compressed
                framed.extend_from_slice(&len.to_be_bytes());
                framed.extend_from_slice(&response_json);

                let mut res = http::Response::new(Full::new(Bytes::from(framed)));
                *res.status_mut() = http::StatusCode::OK;
                res.headers_mut().insert(
                    "content-type",
                    http::HeaderValue::from_static("application/grpc+json"),
                );
                res.headers_mut().insert(
                    "grpc-status",
                    grpc_code.to_string().parse().unwrap(),
                );
                Ok(res)
            })
        }
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = TonicSvc;
            tokio::spawn(async move {
                let _ = hyper_util::server::conn::auto::Builder::new(
                    hyper_util::rt::TokioExecutor::new(),
                )
                .serve_connection(io, hyper_util::service::TowerToHyperService::new(svc))
                .await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

// ---------------------------------------------------------------------------
// gRPC request helper
// ---------------------------------------------------------------------------

async fn grpc_call(client: &reqwest::Client, port: u16, service: &str, method: &str, body: &[u8]) -> Vec<u8> {
    let framed = typeway_grpc::framing::encode_grpc_frame(body);
    let resp = client
        .post(format!("http://127.0.0.1:{port}/{service}/{method}"))
        .header("content-type", "application/grpc+json")
        .header("te", "trailers")
        .body(framed)
        .send()
        .await
        .unwrap();
    resp.bytes().await.unwrap().to_vec()
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_e2e(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let tw_port = rt.block_on(start_typeway_server());
    let baseline_port = rt.block_on(start_tonic_server());

    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .pool_max_idle_per_host(1)
        .build()
        .unwrap();

    let create_body = serde_json::json!({"name": "Charlie"}).to_string().into_bytes();
    let empty_body = b"{}".to_vec();

    let mut group = c.benchmark_group("grpc_e2e");

    // Unary: CreateUser
    group.bench_function("create_user/typeway", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            let body = create_body.clone();
            async move {
                grpc_call(&client, tw_port, "bench.v1.BenchService", "CreateUser", &body).await
            }
        })
    });

    group.bench_function("create_user/baseline", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            let body = create_body.clone();
            async move {
                grpc_call(&client, baseline_port, "bench.v1.BenchService", "CreateUser", &body).await
            }
        })
    });

    // Unary: ListUser
    group.bench_function("list_users/typeway", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            let body = empty_body.clone();
            async move {
                grpc_call(&client, tw_port, "bench.v1.BenchService", "ListUser", &body).await
            }
        })
    });

    group.bench_function("list_users/baseline", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            let body = empty_body.clone();
            async move {
                grpc_call(&client, baseline_port, "bench.v1.BenchService", "ListUser", &body).await
            }
        })
    });

    group.finish();
}

criterion_group!(benches, bench_e2e);
criterion_main!(benches);
