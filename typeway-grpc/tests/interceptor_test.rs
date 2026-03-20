//! Tests for gRPC client interceptors and configuration.

#![cfg(feature = "client")]

use std::time::Duration;

use typeway_grpc::interceptors::GrpcClientConfig;

#[test]
fn default_config_has_30s_timeout() {
    let config = GrpcClientConfig::default();
    assert_eq!(config.timeout, Some(Duration::from_secs(30)));
    assert!(config.default_metadata.is_empty());
    assert!(config.interceptors.is_empty());
}

#[test]
fn bearer_auth_adds_metadata() {
    let config = GrpcClientConfig::default().bearer_auth("my-secret-token");
    assert_eq!(config.default_metadata.len(), 1);
    assert_eq!(config.default_metadata[0].0, "authorization");
    assert_eq!(config.default_metadata[0].1, "Bearer my-secret-token");
}

#[test]
fn metadata_builder_chains() {
    let config = GrpcClientConfig::default()
        .metadata("x-request-id", "abc123")
        .metadata("x-tenant", "acme")
        .bearer_auth("token");
    assert_eq!(config.default_metadata.len(), 3);
    assert_eq!(config.default_metadata[0], ("x-request-id".to_string(), "abc123".to_string()));
    assert_eq!(config.default_metadata[1], ("x-tenant".to_string(), "acme".to_string()));
    assert_eq!(config.default_metadata[2], ("authorization".to_string(), "Bearer token".to_string()));
}

#[test]
fn no_timeout_disables_timeout() {
    let config = GrpcClientConfig::default().no_timeout();
    assert_eq!(config.timeout, None);
}

#[test]
fn custom_timeout() {
    let config = GrpcClientConfig::default().timeout(Duration::from_secs(5));
    assert_eq!(config.timeout, Some(Duration::from_secs(5)));
}

#[test]
fn config_is_debug_printable() {
    let config = GrpcClientConfig::default()
        .metadata("x-test", "value")
        .interceptor(|req| req.header("x-intercepted", "true"));
    let debug = format!("{:?}", config);
    assert!(debug.contains("GrpcClientConfig"));
    assert!(debug.contains("x-test"));
    assert!(debug.contains("1 interceptors"));
}

#[test]
fn interceptors_accumulate() {
    let config = GrpcClientConfig::default()
        .interceptor(|req| req.header("x-first", "1"))
        .interceptor(|req| req.header("x-second", "2"));
    assert_eq!(config.interceptors.len(), 2);
}

#[test]
fn config_is_cloneable() {
    let config = GrpcClientConfig::default()
        .metadata("key", "value")
        .interceptor(|req| req);
    let cloned = config.clone();
    assert_eq!(cloned.default_metadata.len(), 1);
    assert_eq!(cloned.interceptors.len(), 1);
    assert_eq!(cloned.timeout, config.timeout);
}
