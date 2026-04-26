//! gRPC Server Reflection service.
//!
//! Implements a simplified version of the gRPC Server Reflection Protocol
//! (`grpc.reflection.v1alpha.ServerReflection`). This allows gRPC clients
//! to discover available services at runtime (e.g., `grpcurl -plaintext
//! localhost:3000 list`).
//!
//! # How it works
//!
//! The reflection service responds to two query types:
//! - **`list_services`** returns the names of all registered services.
//! - **`file_containing_symbol`** returns the `.proto` file content
//!   for the service containing the requested symbol.
//!
//! # Example
//!
//! ```ignore
//! use typeway_grpc::reflection::ReflectionService;
//! use typeway_grpc::{ApiToProto, ApiToServiceDescriptor};
//!
//! let reflection = ReflectionService::from_api::<MyAPI>("UserService", "users.v1");
//! assert_eq!(reflection.list_services(), &["users.v1.UserService"]);
//! ```

use crate::proto_gen::ApiToProto;
use crate::service::ApiToServiceDescriptor;

/// The well-known gRPC path for the reflection service.
pub const REFLECTION_SERVICE_PATH: &str =
    "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo";

/// The path prefix used by the reflection service.
pub const REFLECTION_SERVICE_PREFIX: &str = "/grpc.reflection.v1alpha";

/// A service reflection handler that responds to gRPC reflection requests.
///
/// This is a simplified implementation of the gRPC Server Reflection Protocol
/// (`grpc.reflection.v1alpha.ServerReflection`). It responds with the service
/// name and proto file content when queried.
#[derive(Debug, Clone)]
pub struct ReflectionService {
    /// The generated `.proto` file content.
    proto_content: String,
    /// Service names (e.g., `["users.v1.UserService"]`).
    service_names: Vec<String>,
}

impl ReflectionService {
    /// Create a reflection service from an API type.
    ///
    /// Generates the `.proto` file and service descriptor from the type-level
    /// API definition. The `service_name` and `package` are used to construct
    /// the fully-qualified service name.
    pub fn from_api<A: ApiToProto + ApiToServiceDescriptor>(
        service_name: &str,
        package: &str,
    ) -> Self {
        let proto = A::to_proto(service_name, package);
        let full_name = format!("{}.{}", package, service_name);
        ReflectionService {
            proto_content: proto,
            service_names: vec![full_name],
        }
    }

    /// Create a reflection service with explicit proto content and service names.
    pub fn new(proto_content: String, service_names: Vec<String>) -> Self {
        ReflectionService {
            proto_content,
            service_names,
        }
    }

    /// List all available service names.
    pub fn list_services(&self) -> &[String] {
        &self.service_names
    }

    /// Get the proto file content for a given symbol.
    ///
    /// This simplified implementation returns the single proto file regardless
    /// of which symbol is requested, since typeway generates one proto file
    /// per service.
    pub fn file_containing_symbol(&self, _symbol: &str) -> Option<&str> {
        Some(&self.proto_content)
    }

    /// Get the raw proto file content.
    pub fn proto_content(&self) -> &str {
        &self.proto_content
    }

    /// Handle a reflection request and return a JSON response.
    ///
    /// Supports two query types:
    /// - `list_services` returns service names
    /// - `file_containing_symbol` returns proto file content
    pub fn handle_request(&self, request_body: &str) -> String {
        if request_body.contains("list_services") {
            let services: Vec<String> = self
                .service_names
                .iter()
                .map(|s| format!("{{\"name\":\"{}\"}}", s))
                .collect();
            format!(
                "{{\"listServicesResponse\":{{\"service\":[{}]}}}}",
                services.join(",")
            )
        } else {
            // Return the proto file content as a JSON-wrapped string.
            let escaped = self
                .proto_content
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n");
            format!(
                "{{\"fileDescriptorResponse\":{{\"fileDescriptorProto\":\"{}\"}}}}",
                escaped
            )
        }
    }

    /// Check if a request path is a reflection service path.
    pub fn is_reflection_path(path: &str) -> bool {
        path == REFLECTION_SERVICE_PATH || path.starts_with(REFLECTION_SERVICE_PREFIX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_reflection() -> ReflectionService {
        ReflectionService::new(
            "syntax = \"proto3\";\npackage test.v1;\nservice TestService {}".to_string(),
            vec!["test.v1.TestService".to_string()],
        )
    }

    #[test]
    fn list_services_returns_service_names() {
        let svc = test_reflection();
        assert_eq!(svc.list_services(), &["test.v1.TestService"]);
    }

    #[test]
    fn file_containing_symbol_returns_proto() {
        let svc = test_reflection();
        let proto = svc.file_containing_symbol("test.v1.TestService");
        assert!(proto.is_some());
        assert!(proto.unwrap().contains("TestService"));
    }

    #[test]
    fn handle_request_list_services() {
        let svc = test_reflection();
        let response = svc.handle_request("{\"list_services\":\"\"}");
        assert!(response.contains("listServicesResponse"));
        assert!(response.contains("test.v1.TestService"));
    }

    #[test]
    fn handle_request_file_descriptor() {
        let svc = test_reflection();
        let response = svc.handle_request("{\"file_containing_symbol\":\"test.v1.TestService\"}");
        assert!(response.contains("fileDescriptorResponse"));
        assert!(response.contains("TestService"));
    }

    #[test]
    fn is_reflection_path_matches() {
        assert!(ReflectionService::is_reflection_path(
            REFLECTION_SERVICE_PATH
        ));
        assert!(ReflectionService::is_reflection_path(
            "/grpc.reflection.v1alpha/foo"
        ));
    }

    #[test]
    fn is_reflection_path_rejects_other_paths() {
        assert!(!ReflectionService::is_reflection_path(
            "/users.v1.UserService/GetUser"
        ));
        assert!(!ReflectionService::is_reflection_path(
            "/grpc.health.v1.Health/Check"
        ));
    }

    #[test]
    fn proto_content_accessor() {
        let svc = test_reflection();
        assert!(svc.proto_content().contains("proto3"));
    }
}
