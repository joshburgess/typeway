//! gRPC Health Check service.
//!
//! Implements the standard `grpc.health.v1.Health/Check` service. The health
//! status can be toggled at runtime for graceful shutdown or load balancer
//! draining.
//!
//! # Example
//!
//! ```
//! use typeway_grpc::health::HealthService;
//!
//! let health = HealthService::new();
//! assert_eq!(health.check(), typeway_grpc::health::HealthStatus::Serving);
//!
//! // Signal graceful shutdown:
//! health.set_not_serving();
//! assert_eq!(health.check(), typeway_grpc::health::HealthStatus::NotServing);
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The well-known gRPC path for the health check service.
pub const HEALTH_SERVICE_PATH: &str = "/grpc.health.v1.Health/Check";

/// The path prefix used by the health check service.
pub const HEALTH_SERVICE_PREFIX: &str = "/grpc.health.v1";

/// A gRPC health check service implementing `grpc.health.v1.Health`.
///
/// Responds to `Check` RPCs with `SERVING` or `NOT_SERVING` status.
/// The serving status can be toggled at runtime for graceful shutdown
/// and load balancer draining.
///
/// This type is cheaply cloneable (internally uses `Arc<AtomicBool>`),
/// so the server can hand out clones to shutdown hooks while retaining
/// a copy for request handling.
#[derive(Clone, Debug)]
pub struct HealthService {
    serving: Arc<AtomicBool>,
}

impl HealthService {
    /// Create a new health service with status `SERVING`.
    pub fn new() -> Self {
        HealthService {
            serving: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Check the current health status.
    pub fn check(&self) -> HealthStatus {
        if self.serving.load(Ordering::Relaxed) {
            HealthStatus::Serving
        } else {
            HealthStatus::NotServing
        }
    }

    /// Set the serving status to `NOT_SERVING`.
    ///
    /// Typically called during graceful shutdown so that load balancers
    /// stop routing traffic to this instance.
    pub fn set_not_serving(&self) {
        self.serving.store(false, Ordering::Relaxed);
    }

    /// Set the serving status back to `SERVING`.
    pub fn set_serving(&self) {
        self.serving.store(true, Ordering::Relaxed);
    }

    /// Handle a health check gRPC request and return a JSON response.
    ///
    /// Returns a JSON object with a `"status"` field set to either
    /// `"SERVING"` or `"NOT_SERVING"`.
    pub fn handle_request(&self) -> String {
        let status = match self.check() {
            HealthStatus::Serving => "SERVING",
            HealthStatus::NotServing => "NOT_SERVING",
        };
        format!("{{\"status\":\"{}\"}}", status)
    }

    /// Check if a request path is a health check service path.
    pub fn is_health_path(path: &str) -> bool {
        path == HEALTH_SERVICE_PATH || path.starts_with(HEALTH_SERVICE_PREFIX)
    }
}

impl Default for HealthService {
    fn default() -> Self {
        Self::new()
    }
}

/// The health status of a gRPC service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HealthStatus {
    /// The service is healthy and accepting requests.
    Serving,
    /// The service is not accepting requests (e.g., shutting down).
    NotServing,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Serving => f.write_str("SERVING"),
            HealthStatus::NotServing => f.write_str("NOT_SERVING"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_status_is_serving() {
        let svc = HealthService::new();
        assert_eq!(svc.check(), HealthStatus::Serving);
    }

    #[test]
    fn set_not_serving_changes_status() {
        let svc = HealthService::new();
        svc.set_not_serving();
        assert_eq!(svc.check(), HealthStatus::NotServing);
    }

    #[test]
    fn set_serving_restores_status() {
        let svc = HealthService::new();
        svc.set_not_serving();
        assert_eq!(svc.check(), HealthStatus::NotServing);
        svc.set_serving();
        assert_eq!(svc.check(), HealthStatus::Serving);
    }

    #[test]
    fn handle_request_serving() {
        let svc = HealthService::new();
        let response = svc.handle_request();
        assert_eq!(response, "{\"status\":\"SERVING\"}");
    }

    #[test]
    fn handle_request_not_serving() {
        let svc = HealthService::new();
        svc.set_not_serving();
        let response = svc.handle_request();
        assert_eq!(response, "{\"status\":\"NOT_SERVING\"}");
    }

    #[test]
    fn clone_shares_state() {
        let svc1 = HealthService::new();
        let svc2 = svc1.clone();
        svc1.set_not_serving();
        assert_eq!(svc2.check(), HealthStatus::NotServing);
    }

    #[test]
    fn default_impl() {
        let svc = HealthService::default();
        assert_eq!(svc.check(), HealthStatus::Serving);
    }

    #[test]
    fn is_health_path_matches() {
        assert!(HealthService::is_health_path(HEALTH_SERVICE_PATH));
        assert!(HealthService::is_health_path("/grpc.health.v1/Watch"));
    }

    #[test]
    fn is_health_path_rejects_other() {
        assert!(!HealthService::is_health_path(
            "/users.v1.UserService/GetUser"
        ));
        assert!(!HealthService::is_health_path(
            "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo"
        ));
    }

    #[test]
    fn health_status_display() {
        assert_eq!(format!("{}", HealthStatus::Serving), "SERVING");
        assert_eq!(format!("{}", HealthStatus::NotServing), "NOT_SERVING");
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn health_service_is_send_sync() {
        _assert_send_sync::<HealthService>();
    }
}
