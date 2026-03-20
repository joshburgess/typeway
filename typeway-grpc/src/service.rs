//! gRPC service descriptor generation from API types.
//!
//! [`GrpcServiceDescriptor`] describes a complete gRPC service at runtime,
//! including all method names, paths, and HTTP method mappings.
//! [`ApiToServiceDescriptor`] builds a descriptor by reusing the
//! [`CollectRpcs`](crate::proto_gen::CollectRpcs) machinery.

use crate::proto_gen::CollectRpcs;

/// Describes a single gRPC method at runtime.
#[derive(Debug, Clone)]
pub struct GrpcMethodDescriptor {
    /// The gRPC method name (PascalCase, e.g., `"GetUser"`).
    pub name: String,
    /// The full gRPC path (e.g., `"/users.v1.UserService/GetUser"`).
    pub full_path: String,
    /// The HTTP method this maps to.
    pub http_method: http::Method,
    /// The REST path pattern (e.g., `"/users/{}"`).
    pub rest_path: String,
}

/// Describes a complete gRPC service.
#[derive(Debug, Clone)]
pub struct GrpcServiceDescriptor {
    /// Service name (e.g., `"UserService"`).
    pub name: String,
    /// Package name (e.g., `"users.v1"`).
    pub package: String,
    /// All methods in the service.
    pub methods: Vec<GrpcMethodDescriptor>,
}

impl GrpcServiceDescriptor {
    /// Look up a method descriptor by its full gRPC path.
    pub fn find_method(&self, full_path: &str) -> Option<&GrpcMethodDescriptor> {
        self.methods.iter().find(|m| m.full_path == full_path)
    }
}

/// Build a [`GrpcServiceDescriptor`] from an API type.
///
/// This reuses [`CollectRpcs`] to walk the API tuple and convert each
/// endpoint into a [`GrpcMethodDescriptor`].
///
/// # Example
///
/// ```ignore
/// use typeway_grpc::service::ApiToServiceDescriptor;
///
/// let desc = MyAPI::service_descriptor("UserService", "users.v1");
/// for method in &desc.methods {
///     println!("{} -> {} {}", method.full_path, method.http_method, method.rest_path);
/// }
/// ```
pub trait ApiToServiceDescriptor: CollectRpcs {
    /// Build the service descriptor for this API.
    fn service_descriptor(service_name: &str, package: &str) -> GrpcServiceDescriptor;
}

impl<T: CollectRpcs> ApiToServiceDescriptor for T {
    fn service_descriptor(service_name: &str, package: &str) -> GrpcServiceDescriptor {
        let rpcs = T::collect_rpcs();
        GrpcServiceDescriptor {
            name: service_name.to_string(),
            package: package.to_string(),
            methods: rpcs
                .iter()
                .map(|rpc| GrpcMethodDescriptor {
                    name: rpc.name.clone(),
                    full_path: format!("/{}.{}/{}", package, service_name, rpc.name),
                    http_method: rpc.http_method.parse::<http::Method>().unwrap_or_else(
                        |_| {
                            panic!(
                                "invalid HTTP method '{}' for gRPC method '{}' \
                                 — this is a bug in proto generation",
                                rpc.http_method, rpc.name
                            )
                        },
                    ),
                    rest_path: rpc.path_pattern.clone(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto_gen::{ProtoMessage, RpcMethod};

    /// A helper that directly constructs a descriptor from RpcMethods
    /// to test the conversion logic without needing full endpoint types.
    fn descriptor_from_rpcs(rpcs: Vec<RpcMethod>) -> GrpcServiceDescriptor {
        GrpcServiceDescriptor {
            name: "TestService".to_string(),
            package: "test.v1".to_string(),
            methods: rpcs
                .iter()
                .map(|rpc| GrpcMethodDescriptor {
                    name: rpc.name.clone(),
                    full_path: format!("/test.v1.TestService/{}", rpc.name),
                    http_method: rpc.http_method.parse::<http::Method>().unwrap_or_else(
                        |_| {
                            panic!(
                                "invalid HTTP method '{}' for gRPC method '{}' \
                                 — this is a bug in proto generation",
                                rpc.http_method, rpc.name
                            )
                        },
                    ),
                    rest_path: rpc.path_pattern.clone(),
                })
                .collect(),
        }
    }

    fn make_rpc(name: &str, method: &str, path: &str) -> RpcMethod {
        RpcMethod {
            name: name.to_string(),
            http_method: method.to_string(),
            path_pattern: path.to_string(),
            request_message: None,
            response_message: ProtoMessage {
                name: format!("{}Response", name),
                definition: String::new(),
            },
            server_streaming: false,
            client_streaming: false,
        }
    }

    #[test]
    fn descriptor_has_correct_service_info() {
        let desc = descriptor_from_rpcs(vec![make_rpc("GetUser", "GET", "/users/{}")]);
        assert_eq!(desc.name, "TestService");
        assert_eq!(desc.package, "test.v1");
        assert_eq!(desc.methods.len(), 1);
    }

    #[test]
    fn method_full_path_format() {
        let desc = descriptor_from_rpcs(vec![
            make_rpc("ListUser", "GET", "/users"),
            make_rpc("GetUser", "GET", "/users/{}"),
        ]);
        assert_eq!(desc.methods[0].full_path, "/test.v1.TestService/ListUser");
        assert_eq!(desc.methods[1].full_path, "/test.v1.TestService/GetUser");
    }

    #[test]
    fn http_method_mapping() {
        let desc = descriptor_from_rpcs(vec![
            make_rpc("ListUser", "GET", "/users"),
            make_rpc("CreateUser", "POST", "/users"),
            make_rpc("UpdateUser", "PUT", "/users/{}"),
            make_rpc("DeleteUser", "DELETE", "/users/{}"),
        ]);
        assert_eq!(desc.methods[0].http_method, http::Method::GET);
        assert_eq!(desc.methods[1].http_method, http::Method::POST);
        assert_eq!(desc.methods[2].http_method, http::Method::PUT);
        assert_eq!(desc.methods[3].http_method, http::Method::DELETE);
    }

    #[test]
    fn rest_path_preserved() {
        let desc = descriptor_from_rpcs(vec![
            make_rpc("GetUser", "GET", "/users/{}"),
            make_rpc("ListPost", "GET", "/users/{}/posts"),
        ]);
        assert_eq!(desc.methods[0].rest_path, "/users/{}");
        assert_eq!(desc.methods[1].rest_path, "/users/{}/posts");
    }

    #[test]
    fn find_method_by_path() {
        let desc = descriptor_from_rpcs(vec![
            make_rpc("ListUser", "GET", "/users"),
            make_rpc("GetUser", "GET", "/users/{}"),
        ]);
        let found = desc.find_method("/test.v1.TestService/GetUser");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "GetUser");

        let not_found = desc.find_method("/test.v1.TestService/DeleteUser");
        assert!(not_found.is_none());
    }
}
