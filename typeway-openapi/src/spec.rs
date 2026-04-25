//! OpenAPI 3.1 specification types.
//!
//! Rust structs mirroring the subset of the OpenAPI spec needed for
//! route, parameter, request body, and response generation.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// A complete OpenAPI 3.1 specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiSpec {
    pub openapi: String,
    pub info: Info,
    pub paths: IndexMap<String, PathItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
}

impl OpenApiSpec {
    /// Create a new spec with the given title and version.
    pub fn new(title: impl Into<String>, version: impl Into<String>) -> Self {
        OpenApiSpec {
            openapi: "3.1.0".to_string(),
            info: Info {
                title: title.into(),
                version: version.into(),
                description: None,
            },
            paths: IndexMap::new(),
            components: None,
        }
    }
}

/// API metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// All operations on a single path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Operation>,
}

impl PathItem {
    /// Set an operation on this path item by HTTP method.
    pub fn set_operation(&mut self, method: &http::Method, op: Operation) {
        match method.as_str() {
            "GET" => self.get = Some(op),
            "POST" => self.post = Some(op),
            "PUT" => self.put = Some(op),
            "DELETE" => self.delete = Some(op),
            "PATCH" => self.patch = Some(op),
            "HEAD" => self.head = Some(op),
            "OPTIONS" => self.options = Some(op),
            _ => {}
        }
    }
}

/// A single API operation (one method on one path).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "operationId", skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    #[serde(rename = "requestBody", skip_serializing_if = "Option::is_none")]
    pub request_body: Option<RequestBody>,
    pub responses: IndexMap<String, Response>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<SecurityRequirement>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
}

impl Operation {
    /// Create a minimal operation with a 200 response.
    pub fn new() -> Self {
        Operation {
            summary: None,
            description: None,
            operation_id: None,
            tags: Vec::new(),
            parameters: Vec::new(),
            request_body: None,
            responses: IndexMap::new(),
            security: Vec::new(),
            deprecated: false,
        }
    }
}

impl Default for Operation {
    fn default() -> Self {
        Self::new()
    }
}

/// A request or response parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: ParameterLocation,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
}

/// Where a parameter is located.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterLocation {
    Query,
    Path,
    Header,
    Cookie,
}

/// A request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBody {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    pub content: IndexMap<String, MediaType>,
}

/// A response description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub description: String,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub content: IndexMap<String, MediaType>,
}

/// A media type with its schema and optional example value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaType {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
}

/// A simplified JSON Schema representation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Schema {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none", default)]
    pub schema_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub items: Option<Box<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub properties: Option<IndexMap<String, Schema>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    /// `oneOf` variants, used to represent Rust enum sum types.
    #[serde(rename = "oneOf", skip_serializing_if = "Option::is_none", default)]
    pub one_of: Option<Vec<Schema>>,
    /// String enumeration values, used for unit-only Rust enums.
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none", default)]
    pub enum_values: Option<Vec<serde_json::Value>>,
    /// Discriminator hint for tagged unions (`oneOf` schemas).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub discriminator: Option<Discriminator>,
}

/// OpenAPI 3 discriminator object for tagged `oneOf` unions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Discriminator {
    #[serde(rename = "propertyName")]
    pub property_name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub mapping: Option<IndexMap<String, String>>,
}

// ---------------------------------------------------------------------------
// Security types
// ---------------------------------------------------------------------------

/// Security requirement on an operation.
///
/// Maps security scheme names to lists of scopes. For bearer auth,
/// the scope list is typically empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRequirement(pub IndexMap<String, Vec<String>>);

impl SecurityRequirement {
    /// Create a bearer authentication security requirement.
    pub fn bearer() -> Self {
        let mut map = IndexMap::new();
        map.insert("bearerAuth".to_string(), Vec::new());
        SecurityRequirement(map)
    }
}

/// Top-level components section of an OpenAPI spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Components {
    #[serde(
        rename = "securitySchemes",
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub security_schemes: IndexMap<String, SecurityScheme>,
}

/// A security scheme definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScheme {
    #[serde(rename = "type")]
    pub scheme_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(rename = "bearerFormat", skip_serializing_if = "Option::is_none")]
    pub bearer_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl SecurityScheme {
    /// Create a bearer token security scheme.
    pub fn bearer_jwt() -> Self {
        SecurityScheme {
            scheme_type: "http".to_string(),
            scheme: Some("bearer".to_string()),
            bearer_format: Some("JWT".to_string()),
            description: Some("Bearer token authentication".to_string()),
        }
    }
}

impl PathItem {
    /// Return mutable references to all operations defined on this path.
    pub fn all_operations_mut(&mut self) -> Vec<&mut Operation> {
        let mut ops = Vec::new();
        if let Some(ref mut op) = self.get {
            ops.push(op);
        }
        if let Some(ref mut op) = self.post {
            ops.push(op);
        }
        if let Some(ref mut op) = self.put {
            ops.push(op);
        }
        if let Some(ref mut op) = self.delete {
            ops.push(op);
        }
        if let Some(ref mut op) = self.patch {
            ops.push(op);
        }
        if let Some(ref mut op) = self.head {
            ops.push(op);
        }
        if let Some(ref mut op) = self.options {
            ops.push(op);
        }
        ops
    }
}

impl Schema {
    pub fn string() -> Self {
        Schema {
            schema_type: Some("string".into()),
            ..Default::default()
        }
    }

    pub fn integer() -> Self {
        Schema {
            schema_type: Some("integer".into()),
            format: Some("int32".into()),
            ..Default::default()
        }
    }

    pub fn integer64() -> Self {
        Schema {
            schema_type: Some("integer".into()),
            format: Some("int64".into()),
            ..Default::default()
        }
    }

    pub fn number() -> Self {
        Schema {
            schema_type: Some("number".into()),
            ..Default::default()
        }
    }

    pub fn boolean() -> Self {
        Schema {
            schema_type: Some("boolean".into()),
            ..Default::default()
        }
    }

    pub fn array(items: Schema) -> Self {
        Schema {
            schema_type: Some("array".into()),
            items: Some(Box::new(items)),
            ..Default::default()
        }
    }

    pub fn object() -> Self {
        Schema {
            schema_type: Some("object".into()),
            ..Default::default()
        }
    }

    /// Construct a string-valued enum schema.
    ///
    /// Used for Rust enums where every variant is a unit variant. The
    /// generated schema is `{"type": "string", "enum": [...]}`.
    pub fn string_enum<S: Into<String>>(values: impl IntoIterator<Item = S>) -> Self {
        Schema {
            schema_type: Some("string".into()),
            enum_values: Some(
                values
                    .into_iter()
                    .map(|s| serde_json::Value::String(s.into()))
                    .collect(),
            ),
            ..Default::default()
        }
    }

    /// Construct a `oneOf` union schema for tagged sum types.
    ///
    /// `discriminator` is set when the variants share a common tag property
    /// (e.g. serde's internally tagged or adjacently tagged representations).
    pub fn one_of(variants: Vec<Schema>, discriminator: Option<Discriminator>) -> Self {
        Schema {
            one_of: Some(variants),
            discriminator,
            ..Default::default()
        }
    }

    /// Construct an object schema from a list of named property schemas.
    ///
    /// This avoids requiring downstream crates to depend on `indexmap` directly.
    ///
    /// ```ignore
    /// Schema::object_with_properties(
    ///     vec![("id", u32_schema), ("name", string_schema)],
    ///     Some("A user account."),
    /// )
    /// ```
    pub fn object_with_properties(
        properties: Vec<(&str, Schema)>,
        description: Option<&str>,
    ) -> Self {
        let mut props = IndexMap::new();
        for (name, schema) in properties {
            props.insert(name.to_string(), schema);
        }
        Schema {
            schema_type: Some("object".to_string()),
            properties: Some(props),
            description: description.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    /// Return a copy of this schema with the description set.
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }
}
