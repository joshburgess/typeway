//! Binary entry point for the gRPC interop server.
//!
//! Listens on an address (default `127.0.0.1:50051`) and serves
//! `grpc.testing.TestService` over HTTP/2.
//!
//! Run with the upstream `grpc-go` interop client:
//!
//! ```sh
//! cargo run --bin interop-server --release
//! # in another shell:
//! interop_client --server_host=127.0.0.1 --server_port=50051 \
//!     --use_tls=false --test_case=empty_unary
//! ```

use std::net::SocketAddr;

use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;

use typeway_interop::server::TestService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    let addr: SocketAddr = std::env::args()
        .nth(1)
        .as_deref()
        .unwrap_or("127.0.0.1:50051")
        .parse()?;

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("interop server listening on http://{addr}");
    tracing::info!("  service: grpc.testing.TestService");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let svc = TowerToHyperService::new(TestService::new());

        tokio::spawn(async move {
            if let Err(e) = Builder::new(TokioExecutor::new())
                .http2_only()
                .serve_connection(io, svc)
                .await
            {
                tracing::debug!("connection closed: {e}");
            }
        });
    }
}
