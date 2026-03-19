//! Production deployment patterns for typeway servers.
//!
//! This module contains no code — it is a collection of patterns and examples
//! for running typeway in production. Each section includes concrete, copy-pasteable
//! code that you can adapt to your own deployment.
//!
//! # Health checks
//!
//! Every production service needs a liveness probe (`/health`) and a readiness
//! probe (`/ready`). The liveness check confirms the process is running. The
//! readiness check confirms the service can handle traffic (database connected,
//! caches warm, etc.).
//!
//! Define them as regular typeway endpoints:
//!
//! ```ignore
//! use typeway_core::*;
//! use typeway_server::*;
//! use std::sync::Arc;
//! use std::sync::atomic::{AtomicBool, Ordering};
//!
//! // API type includes health endpoints alongside your business routes.
//! type HealthAPI = (
//!     GetEndpoint<path!("health"), String>,
//!     GetEndpoint<path!("ready"), String>,
//! );
//!
//! // Liveness: always returns 200 if the process is alive.
//! async fn health() -> String {
//!     "ok".to_string()
//! }
//!
//! // Readiness: checks dependencies before reporting ready.
//! async fn ready(State(state): State<AppState>) -> (http::StatusCode, String) {
//!     if state.is_ready.load(Ordering::Relaxed) {
//!         (http::StatusCode::OK, "ready".to_string())
//!     } else {
//!         (http::StatusCode::SERVICE_UNAVAILABLE, "not ready".to_string())
//!     }
//! }
//!
//! #[derive(Clone)]
//! struct AppState {
//!     is_ready: Arc<AtomicBool>,
//! }
//! ```
//!
//! Kubernetes probe configuration for the above:
//!
//! ```yaml
//! livenessProbe:
//!   httpGet:
//!     path: /health
//!     port: 3000
//!   initialDelaySeconds: 2
//!   periodSeconds: 10
//! readinessProbe:
//!   httpGet:
//!     path: /ready
//!     port: 3000
//!   initialDelaySeconds: 5
//!   periodSeconds: 5
//! ```
//!
//! # Graceful shutdown
//!
//! Use [`Server::serve_with_shutdown`](crate::Server::serve_with_shutdown) to
//! stop accepting new connections when a shutdown signal arrives. In-flight
//! requests on existing connections are allowed to complete. New TCP connections
//! are refused immediately.
//!
//! ```ignore
//! use typeway_server::Server;
//! use tokio::net::TcpListener;
//!
//! let server = Server::<API>::new(handlers);
//! let listener = TcpListener::bind("0.0.0.0:3000").await?;
//!
//! server.serve_with_shutdown(listener, async {
//!     tokio::signal::ctrl_c().await.ok();
//!     println!("Received Ctrl+C, starting shutdown...");
//! }).await?;
//! ```
//!
//! What happens during shutdown:
//!
//! 1. The shutdown future completes (e.g., `ctrl_c()` fires).
//! 2. The accept loop exits — no new TCP connections are accepted.
//! 3. Already-spawned connection tasks continue running until their
//!    current request/response cycle finishes.
//! 4. Once all spawned tasks complete, the process exits cleanly.
//!
//! If you need a hard deadline on in-flight requests, wrap the serve call
//! with [`tokio::time::timeout`]:
//!
//! ```ignore
//! use std::time::Duration;
//!
//! let result = tokio::time::timeout(
//!     Duration::from_secs(30),
//!     server.serve_with_shutdown(listener, async {
//!         tokio::signal::ctrl_c().await.ok();
//!     }),
//! ).await;
//!
//! match result {
//!     Ok(Ok(())) => println!("Clean shutdown"),
//!     Ok(Err(e)) => eprintln!("Server error: {e}"),
//!     Err(_) => eprintln!("Shutdown timed out after 30s, forcing exit"),
//! }
//! ```
//!
//! # Load balancer draining
//!
//! When deploying behind a load balancer (ALB, NLB, HAProxy, envoy, etc.),
//! you want to drain traffic before shutting down. The pattern:
//!
//! 1. Receive SIGTERM (or other shutdown signal).
//! 2. Set readiness to `false` so the load balancer stops sending new requests.
//! 3. Wait a drain period for the LB to detect the change and reroute traffic.
//! 4. Shut down the server, letting in-flight requests finish.
//!
//! ```ignore
//! use std::sync::Arc;
//! use std::sync::atomic::{AtomicBool, Ordering};
//! use std::time::Duration;
//! use typeway_server::Server;
//! use tokio::net::TcpListener;
//!
//! #[derive(Clone)]
//! struct AppState {
//!     is_ready: Arc<AtomicBool>,
//! }
//!
//! async fn ready(State(state): State<AppState>) -> (http::StatusCode, String) {
//!     if state.is_ready.load(Ordering::Relaxed) {
//!         (http::StatusCode::OK, "ready".to_string())
//!     } else {
//!         (http::StatusCode::SERVICE_UNAVAILABLE, "draining".to_string())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let state = AppState {
//!         is_ready: Arc::new(AtomicBool::new(true)),
//!     };
//!     let is_ready = state.is_ready.clone();
//!
//!     let server = Server::<API>::new((health, ready, /* ...other handlers... */))
//!         .with_state(state);
//!
//!     let listener = TcpListener::bind("0.0.0.0:3000").await?;
//!
//!     server.serve_with_shutdown(listener, async move {
//!         // Wait for SIGTERM (container orchestrators send this).
//!         tokio::signal::ctrl_c().await.ok();
//!
//!         // Step 1: Mark as not ready so the LB stops routing to us.
//!         is_ready.store(false, Ordering::Relaxed);
//!         tracing::info!("Marked as not ready, draining for 15 seconds...");
//!
//!         // Step 2: Wait for the load balancer to notice and drain.
//!         // This should be >= your LB's health check interval.
//!         tokio::time::sleep(Duration::from_secs(15)).await;
//!
//!         tracing::info!("Drain period complete, shutting down.");
//!         // Returning from this future triggers the actual shutdown.
//!     }).await
//! }
//! ```
//!
//! Tune the drain period to match your load balancer's health check interval.
//! For AWS ALB with a 10-second check interval, 15 seconds is a safe drain
//! period. For Kubernetes with a 5-second readiness probe, 10 seconds suffices.
//!
//! # Recommended middleware stack
//!
//! The order of middleware layers matters. Layers are applied outside-in: the
//! first `.layer()` call wraps the outermost layer. Here is a recommended
//! production stack:
//!
//! ```ignore
//! use typeway_server::{Server, SecureHeadersLayer};
//! use tower_http::trace::TraceLayer;
//! use tower_http::cors::CorsLayer;
//! use tower_http::timeout::TimeoutLayer;
//! use tower_http::compression::CompressionLayer;
//! use std::time::Duration;
//!
//! let server = Server::<API>::new(handlers)
//!     .with_state(state)
//!     // 1. SecureHeadersLayer (outermost): adds security headers to every
//!     //    response — X-Content-Type-Options, X-Frame-Options, etc.
//!     //    Applied first so that even error responses get security headers.
//!     .layer(SecureHeadersLayer::new())
//!     // 2. TraceLayer: logs every request/response with timing info.
//!     //    Outside of CORS so preflight requests are also logged.
//!     .layer(TraceLayer::new_for_http())
//!     // 3. CorsLayer: handles preflight OPTIONS requests and sets
//!     //    Access-Control-* headers. Must be outside the timeout layer
//!     //    so preflight responses are not subject to handler timeouts.
//!     .layer(CorsLayer::permissive())
//!     // 4. TimeoutLayer: returns 408 Request Timeout if a handler takes
//!     //    too long. Only applies to actual handler execution, not to
//!     //    preflight or middleware processing above.
//!     .layer(TimeoutLayer::new(Duration::from_secs(30)))
//!     // 5. CompressionLayer (innermost): compresses response bodies.
//!     //    Inside timeout so that compression time counts toward the
//!     //    timeout budget.
//!     .layer(CompressionLayer::new());
//!
//! server.serve("0.0.0.0:3000".parse().unwrap()).await?;
//! ```
//!
//! Adjust to your needs:
//!
//! - **CORS**: Replace `CorsLayer::permissive()` with a restrictive policy
//!   for production. Specify allowed origins, methods, and headers explicitly.
//! - **Timeout**: 30 seconds is a reasonable default. Lower it for APIs with
//!   strict latency SLOs.
//! - **Compression**: If your responses are already compressed (e.g., pre-gzipped
//!   static files), you can omit this or move it outside the timeout layer.
//!
//! # Panic recovery
//!
//! Typeway catches panics in request handlers and converts them to 500 Internal
//! Server Error responses. A panicking handler does not take down the server
//! process — only the individual request fails.
//!
//! The panic message is logged via `tracing::error!` for debugging, but is not
//! exposed to the client (to avoid leaking internal details). The client
//! receives a generic 500 response.
//!
//! This means:
//!
//! - You do not need a separate `CatchPanic` middleware in most cases.
//! - Individual handler bugs are isolated to the request that triggered them.
//! - The server continues accepting and processing other requests normally.
//! - You should still fix panics — they indicate bugs — but they will not
//!   cause cascading failures or downtime.
//!
//! If you use [`std::panic::set_hook`] for custom panic reporting (e.g.,
//! sending to Sentry), it will fire for handler panics as well, giving you
//! full stack traces alongside the typeway error log.
