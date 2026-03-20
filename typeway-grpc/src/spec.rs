//! Structured gRPC service specification — the gRPC equivalent of an OpenAPI spec.
//!
//! [`GrpcServiceSpec`] describes every RPC method, its request/response messages,
//! streaming mode, and metadata in a structured format that can be serialized to
//! JSON or YAML and used to generate documentation.
//!
//! [`ApiToGrpcSpec`] derives a spec from the API type at startup, reusing the
//! same [`CollectRpcs`](crate::proto_gen::CollectRpcs) and
//! [`ApiToProto`](crate::proto_gen::ApiToProto) machinery used for `.proto`
//! generation.
//!
//! # Example
//!
//! ```ignore
//! use typeway_grpc::spec::ApiToGrpcSpec;
//!
//! let spec = MyAPI::grpc_spec("UserService", "users.v1");
//! let json = serde_json::to_string_pretty(&spec).unwrap();
//! println!("{json}");
//! ```

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::proto_gen::{ApiToProto, CollectRpcs};

/// A complete gRPC service specification — the gRPC equivalent of an OpenAPI spec.
///
/// Generated from the API type at startup, this document describes every
/// RPC method, its request/response messages, streaming mode, and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcServiceSpec {
    /// The `.proto` file content.
    pub proto: String,
    /// Service metadata.
    pub service: ServiceInfo,
    /// All RPC methods, keyed by method name.
    pub methods: IndexMap<String, MethodSpec>,
    /// All message definitions, keyed by message name.
    pub messages: IndexMap<String, MessageSpec>,
}

/// Metadata about the gRPC service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Service name (PascalCase, e.g., `"UserService"`).
    pub name: String,
    /// Proto package name (e.g., `"users.v1"`).
    pub package: String,
    /// Fully qualified service name (e.g., `"users.v1.UserService"`).
    pub full_name: String,
    /// Human-readable description of the service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Service version string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Specification of a single RPC method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodSpec {
    /// Method name (PascalCase, e.g., `"GetUser"`).
    pub name: String,
    /// Full gRPC path (e.g., `"/users.v1.UserService/GetUser"`).
    pub full_path: String,
    /// The REST endpoint this maps to (e.g., `"/users/{}"`).
    pub rest_path: String,
    /// HTTP method of the REST endpoint (e.g., `"GET"`, `"POST"`).
    pub http_method: String,
    /// Request message name (e.g., `"GetUserRequest"` or `"google.protobuf.Empty"`).
    pub request_type: String,
    /// Response message name (e.g., `"User"` or `"GetUserResponse"`).
    pub response_type: String,
    /// Whether the server streams responses.
    pub server_streaming: bool,
    /// Whether the client streams requests.
    pub client_streaming: bool,
    /// Human-readable description (from [`HandlerDoc`](typeway_core::HandlerDoc) if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Human-readable summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Tags for grouping methods.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Whether this method requires authentication.
    #[serde(default)]
    pub requires_auth: bool,
}

/// Specification of a protobuf message type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSpec {
    /// Message name (e.g., `"User"`, `"GetUserRequest"`).
    pub name: String,
    /// Fields in this message.
    pub fields: Vec<FieldSpec>,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Specification of a single field within a protobuf message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    /// Field name (snake_case, e.g., `"user_id"`).
    pub name: String,
    /// Protobuf type (e.g., `"string"`, `"uint32"`, `"User"`).
    pub proto_type: String,
    /// Field tag number.
    pub tag: u32,
    /// Whether this field is `repeated`.
    #[serde(default)]
    pub repeated: bool,
    /// Whether this field is `optional`.
    #[serde(default)]
    pub optional: bool,
    /// Whether this field is a `map<K, V>` type.
    #[serde(default)]
    pub is_map: bool,
    /// Map key type, if `is_map` is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_key_type: Option<String>,
    /// Map value type, if `is_map` is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_value_type: Option<String>,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// ApiToGrpcSpec trait
// ---------------------------------------------------------------------------

/// Generate a complete gRPC service specification from an API type.
///
/// This is blanket-implemented for any type that implements both
/// [`ApiToProto`] and [`CollectRpcs`], which covers all valid API types.
///
/// # Example
///
/// ```ignore
/// use typeway_grpc::spec::ApiToGrpcSpec;
///
/// let spec = MyAPI::grpc_spec("UserService", "users.v1");
/// println!("Service: {}", spec.service.full_name);
/// for (name, method) in &spec.methods {
///     println!("  {} {} -> {}", method.http_method, method.rest_path, name);
/// }
/// ```
pub trait ApiToGrpcSpec: ApiToProto + CollectRpcs {
    /// Generate a complete gRPC service specification.
    fn grpc_spec(service_name: &str, package: &str) -> GrpcServiceSpec {
        let proto = Self::to_proto(service_name, package);
        let rpcs = Self::collect_rpcs();

        let mut methods = IndexMap::new();
        let mut messages = IndexMap::new();

        for rpc in &rpcs {
            methods.insert(
                rpc.name.clone(),
                MethodSpec {
                    name: rpc.name.clone(),
                    full_path: format!("/{}.{}/{}", package, service_name, rpc.name),
                    rest_path: rpc.path_pattern.clone(),
                    http_method: rpc.http_method.clone(),
                    request_type: rpc
                        .request_message
                        .as_ref()
                        .map(|m| m.name.clone())
                        .unwrap_or_else(|| "google.protobuf.Empty".to_string()),
                    response_type: rpc.response_message.name.clone(),
                    server_streaming: rpc.server_streaming,
                    client_streaming: rpc.client_streaming,
                    description: None,
                    summary: None,
                    tags: Vec::new(),
                    requires_auth: false,
                },
            );

            // Collect message specs from request.
            if let Some(ref req) = rpc.request_message {
                if !messages.contains_key(&req.name) {
                    messages.insert(req.name.clone(), parse_message_spec(&req.definition));
                }
            }
            // Collect message specs from response.
            if !messages.contains_key(&rpc.response_message.name)
                && rpc.response_message.name != "google.protobuf.Empty"
            {
                messages.insert(
                    rpc.response_message.name.clone(),
                    parse_message_spec(&rpc.response_message.definition),
                );
            }
        }

        GrpcServiceSpec {
            proto,
            service: ServiceInfo {
                name: service_name.to_string(),
                package: package.to_string(),
                full_name: format!("{}.{}", package, service_name),
                description: None,
                version: None,
            },
            methods,
            messages,
        }
    }

    /// Generate a spec with handler documentation applied.
    ///
    /// Handler docs (from the `#[documented_handler]` macro) are matched
    /// to RPC methods by converting the handler's `operation_id` to
    /// PascalCase.
    fn grpc_spec_with_docs(
        service_name: &str,
        package: &str,
        docs: &[typeway_core::HandlerDoc],
    ) -> GrpcServiceSpec {
        let mut spec = Self::grpc_spec(service_name, package);

        for doc in docs {
            // Match by operation_id (handler name) -> RPC name.
            // The RPC name is PascalCase of the handler name.
            let pascal = to_pascal_case(doc.operation_id);
            if let Some(method) = spec.methods.get_mut(&pascal) {
                if !doc.summary.is_empty() {
                    method.summary = Some(doc.summary.to_string());
                }
                if !doc.description.is_empty() {
                    method.description = Some(doc.description.to_string());
                }
                method.tags = doc.tags.iter().map(|s| s.to_string()).collect();
            }
        }

        spec
    }
}

/// Blanket impl: anything that can generate proto can generate a spec.
impl<T: ApiToProto + CollectRpcs> ApiToGrpcSpec for T {}

// ---------------------------------------------------------------------------
// Helper: parse a message definition string into a MessageSpec
// ---------------------------------------------------------------------------

/// Parse a protobuf message definition string into a structured [`MessageSpec`].
///
/// The definition is expected to be in the format produced by
/// [`build_message`](crate::mapping::build_message).
fn parse_message_spec(definition: &str) -> MessageSpec {
    let mut name = String::new();
    let mut fields = Vec::new();
    let mut pending_doc: Option<String> = None;

    for line in definition.lines() {
        let line = line.trim();
        if line.starts_with("message ") && line.ends_with('{') {
            name = line
                .trim_start_matches("message ")
                .trim_end_matches(" {")
                .trim()
                .to_string();
        } else if line.starts_with("//") {
            // Comment line — may be a doc comment for the next field.
            let comment = line.trim_start_matches("//").trim().to_string();
            pending_doc = Some(comment);
        } else if !line.is_empty() && line != "}" {
            if let Some(field) = parse_field_spec(line) {
                let field = if let Some(doc) = pending_doc.take() {
                    FieldSpec {
                        description: Some(doc),
                        ..field
                    }
                } else {
                    field
                };
                fields.push(field);
            } else {
                pending_doc = None;
            }
        }
    }

    MessageSpec {
        name,
        fields,
        description: None,
    }
}

/// Parse a single protobuf field line into a [`FieldSpec`].
///
/// Handles:
/// - `type name = tag;`
/// - `repeated type name = tag;`
/// - `optional type name = tag;`
/// - `map<K, V> name = tag;`
fn parse_field_spec(line: &str) -> Option<FieldSpec> {
    let line = line.trim().trim_end_matches(';').trim();
    if line.is_empty() {
        return None;
    }

    // Handle map fields: `map<K, V> name = tag`
    if line.starts_with("map<") {
        return parse_map_field(line);
    }

    let mut repeated = false;
    let mut optional = false;
    let mut rest = line;

    if rest.starts_with("repeated ") {
        repeated = true;
        rest = rest.trim_start_matches("repeated ").trim();
    } else if rest.starts_with("optional ") {
        optional = true;
        rest = rest.trim_start_matches("optional ").trim();
    }

    // Parse: type name = tag
    let parts: Vec<&str> = rest.splitn(4, ' ').collect();
    if parts.len() < 4 {
        return None;
    }

    let proto_type = parts[0].to_string();
    let name = parts[1].to_string();
    // parts[2] should be "="
    let tag: u32 = parts[3].trim().parse().ok()?;

    Some(FieldSpec {
        name,
        proto_type,
        tag,
        repeated,
        optional,
        is_map: false,
        map_key_type: None,
        map_value_type: None,
        description: None,
    })
}

/// Parse a map field line: `map<K, V> name = tag`
fn parse_map_field(line: &str) -> Option<FieldSpec> {
    // Extract the map type parameters between `<` and `>`
    let angle_start = line.find('<')?;
    let angle_end = line.find('>')?;
    let inner = &line[angle_start + 1..angle_end];

    let mut kv = inner.splitn(2, ',');
    let key_type = kv.next()?.trim().to_string();
    let value_type = kv.next()?.trim().to_string();

    // Parse the rest after `>`
    let after_angle = line[angle_end + 1..].trim();
    let parts: Vec<&str> = after_angle.splitn(3, ' ').collect();
    if parts.len() < 3 {
        return None;
    }

    let name = parts[0].to_string();
    // parts[1] should be "="
    let tag: u32 = parts[2].trim().parse().ok()?;

    Some(FieldSpec {
        name,
        proto_type: "map".to_string(),
        tag,
        repeated: false,
        optional: false,
        is_map: true,
        map_key_type: Some(key_type),
        map_value_type: Some(value_type),
        description: None,
    })
}

// ---------------------------------------------------------------------------
// Helper: convert snake_case to PascalCase
// ---------------------------------------------------------------------------

/// Convert a `snake_case` identifier to `PascalCase`.
///
/// Used to match handler `operation_id` names to RPC method names.
pub fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.collect::<String>()
                }
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_pascal_case_basic() {
        assert_eq!(to_pascal_case("get_user"), "GetUser");
        assert_eq!(to_pascal_case("list_users"), "ListUsers");
        assert_eq!(to_pascal_case("create_user"), "CreateUser");
        assert_eq!(to_pascal_case("hello"), "Hello");
    }

    #[test]
    fn parse_simple_field() {
        let field = parse_field_spec("  string name = 1;").unwrap();
        assert_eq!(field.name, "name");
        assert_eq!(field.proto_type, "string");
        assert_eq!(field.tag, 1);
        assert!(!field.repeated);
        assert!(!field.optional);
        assert!(!field.is_map);
    }

    #[test]
    fn parse_repeated_field() {
        let field = parse_field_spec("  repeated uint32 ids = 2;").unwrap();
        assert_eq!(field.name, "ids");
        assert_eq!(field.proto_type, "uint32");
        assert_eq!(field.tag, 2);
        assert!(field.repeated);
    }

    #[test]
    fn parse_optional_field() {
        let field = parse_field_spec("  optional string email = 3;").unwrap();
        assert_eq!(field.name, "email");
        assert_eq!(field.proto_type, "string");
        assert_eq!(field.tag, 3);
        assert!(field.optional);
    }

    #[test]
    fn parse_map_field_line() {
        let field = parse_field_spec("  map<string, uint32> metadata = 1;").unwrap();
        assert_eq!(field.name, "metadata");
        assert!(field.is_map);
        assert_eq!(field.map_key_type.as_deref(), Some("string"));
        assert_eq!(field.map_value_type.as_deref(), Some("uint32"));
        assert_eq!(field.tag, 1);
    }

    #[test]
    fn parse_message_spec_basic() {
        let def = "message User {\n  uint32 id = 1;\n  string name = 2;\n}";
        let spec = parse_message_spec(def);
        assert_eq!(spec.name, "User");
        assert_eq!(spec.fields.len(), 2);
        assert_eq!(spec.fields[0].name, "id");
        assert_eq!(spec.fields[0].proto_type, "uint32");
        assert_eq!(spec.fields[0].tag, 1);
        assert_eq!(spec.fields[1].name, "name");
        assert_eq!(spec.fields[1].proto_type, "string");
        assert_eq!(spec.fields[1].tag, 2);
    }

    #[test]
    fn parse_message_spec_with_doc_comments() {
        let def = "message Req {\n  // The user identifier\n  string id = 1;\n}";
        let spec = parse_message_spec(def);
        assert_eq!(spec.name, "Req");
        assert_eq!(spec.fields.len(), 1);
        assert_eq!(
            spec.fields[0].description.as_deref(),
            Some("The user identifier")
        );
    }

    #[test]
    fn parse_empty_message() {
        let def = "message Empty {\n}";
        let spec = parse_message_spec(def);
        assert_eq!(spec.name, "Empty");
        assert!(spec.fields.is_empty());
    }

    #[test]
    fn parse_empty_definition_string() {
        let spec = parse_message_spec("");
        assert_eq!(spec.name, "");
        assert!(spec.fields.is_empty());
    }
}
