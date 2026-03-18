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

/// A media type with its schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaType {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
}

/// A simplified JSON Schema representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<IndexMap<String, Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Schema {
    pub fn string() -> Self {
        Schema {
            schema_type: Some("string".into()),
            format: None,
            items: None,
            properties: None,
            description: None,
        }
    }

    pub fn integer() -> Self {
        Schema {
            schema_type: Some("integer".into()),
            format: Some("int32".into()),
            items: None,
            properties: None,
            description: None,
        }
    }

    pub fn integer64() -> Self {
        Schema {
            schema_type: Some("integer".into()),
            format: Some("int64".into()),
            items: None,
            properties: None,
            description: None,
        }
    }

    pub fn number() -> Self {
        Schema {
            schema_type: Some("number".into()),
            format: None,
            items: None,
            properties: None,
            description: None,
        }
    }

    pub fn boolean() -> Self {
        Schema {
            schema_type: Some("boolean".into()),
            format: None,
            items: None,
            properties: None,
            description: None,
        }
    }

    pub fn array(items: Schema) -> Self {
        Schema {
            schema_type: Some("array".into()),
            format: None,
            items: Some(Box::new(items)),
            properties: None,
            description: None,
        }
    }

    pub fn object() -> Self {
        Schema {
            schema_type: Some("object".into()),
            format: None,
            items: None,
            properties: None,
            description: None,
        }
    }
}
