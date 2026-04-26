//! End-to-end gRPC benchmark: typeway-grpc vs Tonic.
//!
//! Measures real request latency through the full stack:
//! HTTP/2 → gRPC frame decode → dispatch → handler → encode → HTTP/2 response.
//!
//! Three servers handling the same CreateUser RPC:
//! 1. **typeway** — native dispatch with GrpcMultiplexer
//! 2. **tonic** — real tonic::transport::Server with prost encode/decode
//! 3. **baseline** — bare hyper, zero framework overhead (theoretical floor)
//!
//! Run with: `cargo bench --bench grpc_e2e --features grpc`

use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use serde::{Deserialize, Serialize};

use typeway_core::endpoint::{GetEndpoint, PostEndpoint};
use typeway_core::path::{HCons, HNil, Lit, LitSegment};
use typeway_grpc::mapping::ToProtoType;
use typeway_macros::TypewayCodec;
use typeway_server::*;

// =========================================================================
// Shared types
// =========================================================================

#[allow(non_camel_case_types)]
struct __lit_users;
impl LitSegment for __lit_users {
    const VALUE: &'static str = "users";
}
type UsersPath = HCons<Lit<__lit_users>, HNil>;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec)]
struct User {
    #[proto(tag = 1)]
    id: u32,
    #[proto(tag = 2)]
    name: String,
}
impl ToProtoType for User {
    fn proto_type_name() -> &'static str {
        "User"
    }
    fn is_message() -> bool {
        true
    }
    fn message_definition() -> Option<String> {
        Some("message User {\n  uint32 id = 1;\n  string name = 2;\n}".to_string())
    }
}

#[derive(Debug, Default, Serialize, Deserialize, TypewayCodec)]
struct CreateUser {
    #[proto(tag = 1)]
    name: String,
}
impl ToProtoType for CreateUser {
    fn proto_type_name() -> &'static str {
        "CreateUser"
    }
    fn is_message() -> bool {
        true
    }
    fn message_definition() -> Option<String> {
        Some("message CreateUser {\n  string name = 1;\n}".to_string())
    }
}

// Medium-sized types for larger message benchmark
#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec)]
struct UserProfile {
    #[proto(tag = 1)]
    id: u64,
    #[proto(tag = 2)]
    username: String,
    #[proto(tag = 3)]
    email: String,
    #[proto(tag = 4)]
    bio: String,
    #[proto(tag = 5)]
    active: bool,
    #[proto(tag = 6)]
    score: f64,
    #[proto(tag = 7)]
    level: u32,
}
impl ToProtoType for UserProfile {
    fn proto_type_name() -> &'static str {
        "UserProfile"
    }
    fn is_message() -> bool {
        true
    }
    fn message_definition() -> Option<String> {
        Some("message UserProfile {\n  uint64 id = 1;\n  string username = 2;\n  string email = 3;\n  string bio = 4;\n  bool active = 5;\n  double score = 6;\n  uint32 level = 7;\n}".to_string())
    }
}

#[derive(Clone, PartialEq, prost::Message)]
struct ProstUserProfile {
    #[prost(uint64, tag = "1")]
    id: u64,
    #[prost(string, tag = "2")]
    username: String,
    #[prost(string, tag = "3")]
    email: String,
    #[prost(string, tag = "4")]
    bio: String,
    #[prost(bool, tag = "5")]
    active: bool,
    #[prost(double, tag = "6")]
    score: f64,
    #[prost(uint32, tag = "7")]
    level: u32,
}

#[allow(non_camel_case_types)]
struct __lit_profiles;
impl LitSegment for __lit_profiles {
    const VALUE: &'static str = "profiles";
}
type ProfilesPath = HCons<Lit<__lit_profiles>, HNil>;

// Prost message types for tonic server
#[derive(Clone, PartialEq, prost::Message)]
struct ProstCreateUserRequest {
    #[prost(string, tag = "1")]
    name: String,
}

#[derive(Clone, PartialEq, prost::Message)]
struct ProstUser {
    #[prost(uint32, tag = "1")]
    id: u32,
    #[prost(string, tag = "2")]
    name: String,
}

// =========================================================================
// 1. Typeway server
// =========================================================================

type TestAPI = (
    GetEndpoint<UsersPath, Vec<User>>,
    PostEndpoint<UsersPath, CreateUser, User>,
);

async fn tw_list_users() -> Json<Vec<User>> {
    Json(vec![
        User {
            id: 1,
            name: "Alice".into(),
        },
        User {
            id: 2,
            name: "Bob".into(),
        },
    ])
}

async fn tw_create_user(body: Json<CreateUser>) -> (http::StatusCode, Json<User>) {
    (
        http::StatusCode::CREATED,
        Json(User {
            id: 3,
            name: body.0.name,
        }),
    )
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

// =========================================================================
// 1b. Typeway server with Proto<T> (binary fast path)
// =========================================================================

async fn tw_create_user_proto(body: Proto<CreateUser>) -> Proto<User> {
    Proto(User {
        id: 3,
        name: body.name.clone(),
    })
}

async fn tw_create_profile_proto(body: Proto<UserProfile>) -> Proto<UserProfile> {
    Proto(UserProfile {
        id: body.id + 1,
        username: body.username.clone(),
        email: body.email.clone(),
        bio: body.bio.clone(),
        active: true,
        score: 98.5,
        level: 42,
    })
}

async fn start_typeway_proto_server() -> u16 {
    type ProtoAPI = (PostEndpoint<UsersPath, CreateUser, User>,);

    let server = Server::<ProtoAPI>::new((bind::<_, _, _>(tw_create_user_proto),))
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

// =========================================================================
// 1c. Typeway server with direct handler (maximum throughput)
// =========================================================================

async fn start_typeway_direct_server() -> u16 {
    use typeway_server::grpc_direct::into_direct_handler;

    let direct = into_direct_handler(|req: CreateUser| async move {
        User {
            id: 3,
            name: req.name,
        }
    });

    type DirectAPI = (PostEndpoint<UsersPath, CreateUser, User>,);

    let _server = Server::<DirectAPI>::new((bind::<_, _, _>(tw_create_user),))
        .with_grpc("BenchService", "bench.v1");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let _addr = listener.local_addr().unwrap();
    let descriptor =
        <DirectAPI as typeway_grpc::service::ApiToServiceDescriptor>::service_descriptor(
            "BenchService",
            "bench.v1",
        );
    let mut grpc_router = typeway_server::grpc_dispatch::GrpcRouter::from_router(
        &typeway_server::Router::new(), // empty — direct handler replaces it
        &descriptor,
    );
    grpc_router.add_direct_handler("/bench.v1.BenchService/CreateUser".to_string(), direct);

    // Build multiplexer manually with the direct handler.
    let multiplexer = typeway_server::grpc_dispatch::GrpcMultiplexer::new(
        typeway_server::RouterService::new(std::sync::Arc::new(typeway_server::Router::new())),
        std::sync::Arc::new(grpc_router),
        std::sync::Arc::new(typeway_grpc::ReflectionService::from_api::<DirectAPI>(
            "BenchService",
            "bench.v1",
        )),
        typeway_grpc::HealthService::new(),
        false,
        None,
        None,
    );

    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = multiplexer.clone();
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

// =========================================================================
// 2. Tonic server (manual impl matching what codegen produces)
// =========================================================================

// This mirrors what tonic's codegen generates: an async trait with
// #[async_trait], prost Message encode/decode, tonic::Response wrapping.

#[async_trait::async_trait]
trait BenchServiceTonic: Send + Sync + 'static {
    async fn create_user(
        &self,
        request: tonic::Request<ProstCreateUserRequest>,
    ) -> Result<tonic::Response<ProstUser>, tonic::Status>;
}

#[derive(Clone)]
struct BenchServiceTonicImpl;

#[async_trait::async_trait]
impl BenchServiceTonic for BenchServiceTonicImpl {
    async fn create_user(
        &self,
        request: tonic::Request<ProstCreateUserRequest>,
    ) -> Result<tonic::Response<ProstUser>, tonic::Status> {
        let req = request.into_inner();
        Ok(tonic::Response::new(ProstUser {
            id: 3,
            name: req.name,
        }))
    }
}

// Tower service wrapping the tonic trait (mirrors codegen's generated server)
#[derive(Clone)]
struct TonicBenchServer<T: BenchServiceTonic> {
    inner: T,
}

impl<T: BenchServiceTonic + Clone> tower_service::Service<http::Request<tonic::body::BoxBody>>
    for TonicBenchServer<T>
{
    type Response = http::Response<tonic::body::BoxBody>;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<tonic::body::BoxBody>) -> Self::Future {
        let inner = self.inner.clone();

        Box::pin(async move {
            let path = req.uri().path().to_string();

            if path.ends_with("/CreateUser") {
                // Decode prost message from gRPC frame
                let body_bytes = http_body_util::BodyExt::collect(req.into_body())
                    .await
                    .map(|c| c.to_bytes())
                    .unwrap_or_default();

                let unframed = if body_bytes.len() >= 5 {
                    &body_bytes[5..]
                } else {
                    &body_bytes[..]
                };

                let prost_req = <ProstCreateUserRequest as prost::Message>::decode(unframed)
                    .unwrap_or_default();

                let tonic_req = tonic::Request::new(prost_req);
                let result = inner.create_user(tonic_req).await;

                match result {
                    Ok(response) => {
                        let msg = response.into_inner();
                        let encoded = prost::Message::encode_to_vec(&msg);

                        // gRPC frame
                        let len = encoded.len() as u32;
                        let mut framed = Vec::with_capacity(5 + encoded.len());
                        framed.push(0);
                        framed.extend_from_slice(&len.to_be_bytes());
                        framed.extend_from_slice(&encoded);

                        let body = tonic::body::BoxBody::new(http_body_util::BodyExt::map_err(
                            http_body_util::Full::new(bytes::Bytes::from(framed)),
                            |e| match e {},
                        ));
                        let mut res = http::Response::new(body);
                        *res.status_mut() = http::StatusCode::OK;
                        res.headers_mut().insert(
                            "content-type",
                            http::HeaderValue::from_static("application/grpc+proto"),
                        );
                        res.headers_mut()
                            .insert("grpc-status", http::HeaderValue::from_static("0"));
                        Ok(res)
                    }
                    Err(status) => {
                        let body = tonic::body::BoxBody::new(http_body_util::BodyExt::map_err(
                            http_body_util::Empty::<bytes::Bytes>::new(),
                            |e| match e {},
                        ));
                        let mut res = http::Response::new(body);
                        *res.status_mut() = http::StatusCode::OK;
                        res.headers_mut().insert(
                            "grpc-status",
                            status.code().to_string().as_str().parse().unwrap(),
                        );
                        Ok(res)
                    }
                }
            } else {
                let body = tonic::body::BoxBody::new(http_body_util::BodyExt::map_err(
                    http_body_util::Empty::<bytes::Bytes>::new(),
                    |e| match e {},
                ));
                let mut res = http::Response::new(body);
                *res.status_mut() = http::StatusCode::OK;
                res.headers_mut()
                    .insert("grpc-status", http::HeaderValue::from_static("12"));
                Ok(res)
            }
        })
    }
}

impl<T: BenchServiceTonic> tonic::server::NamedService for TonicBenchServer<T> {
    const NAME: &'static str = "bench.v1.BenchService";
}

async fn start_tonic_server() -> u16 {
    let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let svc = TonicBenchServer {
        inner: BenchServiceTonicImpl,
    };

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(svc)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    port
}

// =========================================================================
// 3. Baseline (bare hyper, zero framework)
// =========================================================================

async fn start_baseline_server() -> u16 {
    use std::convert::Infallible;

    #[derive(Clone)]
    struct BaselineSvc;

    impl tower_service::Service<http::Request<hyper::body::Incoming>> for BaselineSvc {
        type Response = http::Response<http_body_util::Full<bytes::Bytes>>;
        type Error = Infallible;
        type Future = std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
        >;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<hyper::body::Incoming>) -> Self::Future {
            Box::pin(async move {
                let body_bytes = http_body_util::BodyExt::collect(req.into_body())
                    .await
                    .map(|c| c.to_bytes())
                    .unwrap_or_default();

                // Strip gRPC frame, parse JSON, produce JSON response
                let unframed = if body_bytes.len() >= 5 {
                    &body_bytes[5..]
                } else {
                    &body_bytes[..]
                };
                let input: serde_json::Value = serde_json::from_slice(unframed).unwrap_or_default();
                let name = input
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let resp_json =
                    serde_json::to_vec(&serde_json::json!({"id": 3, "name": name})).unwrap();

                // gRPC frame
                let len = resp_json.len() as u32;
                let mut framed = Vec::with_capacity(5 + resp_json.len());
                framed.push(0);
                framed.extend_from_slice(&len.to_be_bytes());
                framed.extend_from_slice(&resp_json);

                let mut res =
                    http::Response::new(http_body_util::Full::new(bytes::Bytes::from(framed)));
                *res.status_mut() = http::StatusCode::OK;
                res.headers_mut().insert(
                    "content-type",
                    http::HeaderValue::from_static("application/grpc+json"),
                );
                res.headers_mut()
                    .insert("grpc-status", http::HeaderValue::from_static("0"));
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
            tokio::spawn(async move {
                let _ = hyper_util::server::conn::auto::Builder::new(
                    hyper_util::rt::TokioExecutor::new(),
                )
                .serve_connection(
                    io,
                    hyper_util::service::TowerToHyperService::new(BaselineSvc),
                )
                .await;
            });
        }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

// =========================================================================
// Benchmark
// =========================================================================

async fn grpc_call(
    client: &reqwest::Client,
    port: u16,
    service: &str,
    method: &str,
    body: &[u8],
) -> Vec<u8> {
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

async fn grpc_call_binary(
    client: &reqwest::Client,
    port: u16,
    service: &str,
    method: &str,
    body: &[u8],
) -> Vec<u8> {
    let framed = typeway_grpc::framing::encode_grpc_frame(body);
    let resp = client
        .post(format!("http://127.0.0.1:{port}/{service}/{method}"))
        .header("content-type", "application/grpc+proto")
        .header("te", "trailers")
        .body(framed)
        .send()
        .await
        .unwrap();
    resp.bytes().await.unwrap().to_vec()
}

fn bench_e2e(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let tw_port = rt.block_on(start_typeway_server());
    let tw_proto_port = rt.block_on(start_typeway_proto_server());
    let tw_direct_port = rt.block_on(start_typeway_direct_server());
    let tonic_port = rt.block_on(start_tonic_server());
    let baseline_port = rt.block_on(start_baseline_server());

    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .pool_max_idle_per_host(1)
        .build()
        .unwrap();

    let create_body = serde_json::json!({"name": "Charlie"})
        .to_string()
        .into_bytes();

    // Binary protobuf body for Proto<T> and Tonic comparisons.
    use typeway_protobuf::TypewayEncode;
    let create_binary = CreateUser {
        name: "Charlie".into(),
    }
    .encode_to_vec();
    let create_prost_binary = prost::Message::encode_to_vec(&ProstCreateUserRequest {
        name: "Charlie".into(),
    });

    let mut group = c.benchmark_group("grpc_e2e");

    group.bench_function("create_user/typeway", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            let body = create_body.clone();
            async move {
                grpc_call(
                    &client,
                    tw_port,
                    "bench.v1.BenchService",
                    "CreateUser",
                    &body,
                )
                .await
            }
        })
    });

    group.bench_function("create_user/typeway_proto", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            let body = create_binary.clone();
            async move {
                grpc_call_binary(
                    &client,
                    tw_proto_port,
                    "bench.v1.BenchService",
                    "CreateUser",
                    &body,
                )
                .await
            }
        })
    });

    group.bench_function("create_user/typeway_direct", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            let body = create_binary.clone();
            async move {
                grpc_call_binary(
                    &client,
                    tw_direct_port,
                    "bench.v1.BenchService",
                    "CreateUser",
                    &body,
                )
                .await
            }
        })
    });

    group.bench_function("create_user/tonic", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            let body = create_prost_binary.clone();
            async move {
                grpc_call_binary(
                    &client,
                    tonic_port,
                    "bench.v1.BenchService",
                    "CreateUser",
                    &body,
                )
                .await
            }
        })
    });

    group.bench_function("create_user/baseline", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            let body = create_body.clone();
            async move {
                grpc_call(
                    &client,
                    baseline_port,
                    "bench.v1.BenchService",
                    "CreateUser",
                    &body,
                )
                .await
            }
        })
    });

    group.finish();
}

fn bench_e2e_medium(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Typeway server with Proto<T> for medium message
    let tw_profile_port = rt.block_on(async {
        type ProfileAPI = (PostEndpoint<ProfilesPath, UserProfile, UserProfile>,);
        let server = Server::<ProfileAPI>::new((bind::<_, _, _>(tw_create_profile_proto),))
            .with_grpc("ProfileService", "bench.v1");
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
    });

    // Tonic server for medium message — same handler logic with prost types
    let tonic_profile_port = rt.block_on(async {
        use std::convert::Infallible;

        #[derive(Clone)]
        struct TonicProfileSvc;

        impl tower_service::Service<http::Request<tonic::body::BoxBody>> for TonicProfileSvc {
            type Response = http::Response<tonic::body::BoxBody>;
            type Error = Infallible;
            type Future = std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
            >;

            fn poll_ready(
                &mut self,
                _cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), Self::Error>> {
                std::task::Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: http::Request<tonic::body::BoxBody>) -> Self::Future {
                Box::pin(async move {
                    let body_bytes = http_body_util::BodyExt::collect(req.into_body())
                        .await
                        .map(|c| c.to_bytes())
                        .unwrap_or_default();
                    let unframed = if body_bytes.len() >= 5 {
                        &body_bytes[5..]
                    } else {
                        &body_bytes[..]
                    };

                    let req_msg =
                        <ProstUserProfile as prost::Message>::decode(unframed).unwrap_or_default();
                    let resp_msg = ProstUserProfile {
                        id: req_msg.id + 1,
                        username: req_msg.username,
                        email: req_msg.email,
                        bio: req_msg.bio,
                        active: true,
                        score: 98.5,
                        level: 42,
                    };

                    let encoded = prost::Message::encode_to_vec(&resp_msg);
                    let len = encoded.len() as u32;
                    let mut framed = Vec::with_capacity(5 + encoded.len());
                    framed.push(0);
                    framed.extend_from_slice(&len.to_be_bytes());
                    framed.extend_from_slice(&encoded);

                    let body = tonic::body::BoxBody::new(http_body_util::BodyExt::map_err(
                        http_body_util::Full::new(bytes::Bytes::from(framed)),
                        |e| match e {},
                    ));
                    let mut res = http::Response::new(body);
                    *res.status_mut() = http::StatusCode::OK;
                    res.headers_mut().insert(
                        "content-type",
                        http::HeaderValue::from_static("application/grpc+proto"),
                    );
                    res.headers_mut()
                        .insert("grpc-status", http::HeaderValue::from_static("0"));
                    Ok(res)
                })
            }
        }

        impl tonic::server::NamedService for TonicProfileSvc {
            const NAME: &'static str = "bench.v1.ProfileService";
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(TonicProfileSvc)
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        port
    });

    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .pool_max_idle_per_host(1)
        .build()
        .unwrap();

    use typeway_protobuf::TypewayEncode;
    let profile_binary = UserProfile {
        id: 12345,
        username: "johndoe".into(),
        email: "john.doe@example.com".into(),
        bio: "Software developer with 10 years of experience in systems programming.".into(),
        active: true,
        score: 98.5,
        level: 42,
    }
    .encode_to_vec();

    let profile_prost_binary = prost::Message::encode_to_vec(&ProstUserProfile {
        id: 12345,
        username: "johndoe".into(),
        email: "john.doe@example.com".into(),
        bio: "Software developer with 10 years of experience in systems programming.".into(),
        active: true,
        score: 98.5,
        level: 42,
    });

    let mut group = c.benchmark_group("grpc_e2e_medium");

    group.bench_function("create_profile/typeway_proto", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            let body = profile_binary.clone();
            async move {
                grpc_call_binary(
                    &client,
                    tw_profile_port,
                    "bench.v1.ProfileService",
                    "CreateProfile",
                    &body,
                )
                .await
            }
        })
    });

    group.bench_function("create_profile/tonic", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            let body = profile_prost_binary.clone();
            async move {
                grpc_call_binary(
                    &client,
                    tonic_profile_port,
                    "bench.v1.ProfileService",
                    "CreateProfile",
                    &body,
                )
                .await
            }
        })
    });

    group.finish();
}

criterion_group!(benches, bench_e2e, bench_e2e_medium);
criterion_main!(benches);
