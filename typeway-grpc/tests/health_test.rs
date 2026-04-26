//! Tests for the gRPC health check service.

use typeway_grpc::health::{HealthService, HealthStatus};

#[test]
fn default_status_is_serving() {
    let svc = HealthService::new();
    assert_eq!(svc.check(), HealthStatus::Serving);
}

#[test]
fn default_impl_is_serving() {
    let svc = HealthService::default();
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
fn handle_request_returns_serving_json() {
    let svc = HealthService::new();
    let response = svc.handle_request();
    assert_eq!(response, "{\"status\":\"SERVING\"}");
}

#[test]
fn handle_request_returns_not_serving_json() {
    let svc = HealthService::new();
    svc.set_not_serving();
    let response = svc.handle_request();
    assert_eq!(response, "{\"status\":\"NOT_SERVING\"}");
}

#[test]
fn clone_shares_state() {
    let svc1 = HealthService::new();
    let svc2 = svc1.clone();
    assert_eq!(svc2.check(), HealthStatus::Serving);

    svc1.set_not_serving();
    assert_eq!(svc2.check(), HealthStatus::NotServing);

    svc2.set_serving();
    assert_eq!(svc1.check(), HealthStatus::Serving);
}

#[test]
fn is_health_path_matches_check() {
    assert!(HealthService::is_health_path(
        "/grpc.health.v1.Health/Check"
    ));
}

#[test]
fn is_health_path_matches_prefix() {
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
    assert!(!HealthService::is_health_path("/"));
}

#[test]
fn health_status_display() {
    assert_eq!(format!("{}", HealthStatus::Serving), "SERVING");
    assert_eq!(format!("{}", HealthStatus::NotServing), "NOT_SERVING");
}

fn _assert_send<T: Send>() {}
fn _assert_sync<T: Sync>() {}
fn _assert_clone<T: Clone>() {}

#[test]
fn health_service_is_send_sync_clone() {
    _assert_send::<HealthService>();
    _assert_sync::<HealthService>();
    _assert_clone::<HealthService>();
}
