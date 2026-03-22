//! OpenAPI 2.x (Swagger) → Typeway Rust codegen.
//!
//! Parses a Swagger 2.0 spec and generates Rust types, paths, and an API
//! type alias compatible with the typeway framework.
//!
//! # Example
//!
//! ```ignore
//! let spec = std::fs::read_to_string("swagger.json").unwrap();
//! let rust_code = typeway_openapi::codegen_v2::swagger_to_typeway(&spec).unwrap();
//! std::fs::write("src/generated.rs", rust_code).unwrap();
//! ```

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::codegen_common::*;

// ---------------------------------------------------------------------------
// Swagger 2.0 AST (minimal subset)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SwaggerSpec {
    swagger: String,
    #[serde(default)]
    info: SwaggerInfo,
    #[serde(default)]
    paths: BTreeMap<String, BTreeMap<String, SwaggerOperation>>,
    #[serde(default)]
    definitions: BTreeMap<String, SwaggerSchema>,
}

#[derive(Debug, Default, Deserialize)]
struct SwaggerInfo {
    #[serde(default)]
    title: String,
    #[serde(default)]
    version: String,
}

#[derive(Debug, Deserialize)]
struct SwaggerOperation {
    #[serde(default)]
    parameters: Vec<SwaggerParameter>,
    #[serde(default)]
    responses: BTreeMap<String, SwaggerResponse>,
    #[serde(default)]
    summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SwaggerParameter {
    #[serde(default)]
    name: String,
    #[serde(rename = "in", default)]
    location: String,
    #[serde(default)]
    schema: Option<SwaggerSchemaRef>,
    #[serde(rename = "type", default)]
    param_type: Option<String>,
    #[serde(default)]
    format: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SwaggerResponse {
    #[serde(default)]
    schema: Option<SwaggerSchemaRef>,
}

#[derive(Debug, Deserialize)]
struct SwaggerSchemaRef {
    #[serde(rename = "$ref")]
    ref_path: Option<String>,
    #[serde(rename = "type")]
    schema_type: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    items: Option<Box<SwaggerSchemaRef>>,
}

#[derive(Debug, Deserialize)]
struct SwaggerSchema {
    #[serde(rename = "type", default)]
    schema_type: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    properties: BTreeMap<String, SwaggerSchemaRef>,
    #[serde(default)]
    items: Option<Box<SwaggerSchemaRef>>,
}

// ---------------------------------------------------------------------------
// Codegen
// ---------------------------------------------------------------------------

/// Generate typeway Rust code from a Swagger 2.0 spec (JSON or YAML).
pub fn swagger_to_typeway(source: &str) -> Result<String, String> {
    let spec: SwaggerSpec = serde_json::from_str(source)
        .or_else(|_| serde_yaml::from_str(source))
        .map_err(|e| format!("failed to parse Swagger spec: {e}"))?;

    if !spec.swagger.starts_with('2') {
        return Err(format!(
            "expected Swagger 2.x, got version '{}'",
            spec.swagger
        ));
    }

    let mut output = String::new();

    // Header.
    output.push_str(&format!(
        "//! Generated from Swagger 2.0 spec: {}\n",
        spec.info.title
    ));
    output.push_str("//! Manual edits may be overwritten.\n\n");
    output.push_str("use typeway::prelude::*;\n");
    output.push_str("use serde::{Serialize, Deserialize};\n\n");

    // Generate structs from definitions.
    for (name, schema) in &spec.definitions {
        let fields = schema_to_fields(schema);
        output.push_str(&generate_struct(name, &fields));
        output.push_str("\n\n");
    }

    // Generate paths and endpoints.
    let mut path_decls = Vec::new();
    let mut endpoint_types = Vec::new();

    for (path, methods) in &spec.paths {
        let path_type_name = path_to_type_name(path);
        let macro_args = path_to_macro_args(path);

        let mut path_declared = false;
        for (method, operation) in methods {
            if !path_declared {
                path_decls.push(format!(
                    "typeway_path!(type {} = {});",
                    path_type_name, macro_args
                ));
                path_declared = true;
            }

            let req_type = find_body_type_v2(operation);
            let res_type = find_response_type_v2(operation);
            let ep = endpoint_type(method, &path_type_name, req_type.as_deref(), &res_type);
            endpoint_types.push(ep);
        }
    }

    // Emit.
    if !path_decls.is_empty() {
        output.push_str("// Path type declarations\n");
        for decl in &path_decls {
            output.push_str(decl);
            output.push('\n');
        }
        output.push('\n');
    }

    if !endpoint_types.is_empty() {
        output.push_str("type API = (\n");
        for ep in &endpoint_types {
            output.push_str(&format!("    {},\n", ep));
        }
        output.push_str(");\n");
    }

    Ok(output)
}

fn schema_to_fields(schema: &SwaggerSchema) -> Vec<(String, String)> {
    schema
        .properties
        .iter()
        .map(|(name, prop)| {
            let rust_type = schema_ref_to_rust(prop);
            (name.clone(), rust_type)
        })
        .collect()
}

fn schema_ref_to_rust(schema: &SwaggerSchemaRef) -> String {
    if let Some(ref ref_path) = schema.ref_path {
        return sanitize_name(&ref_to_name(ref_path));
    }
    if let Some(ref ty) = schema.schema_type {
        if ty == "array" {
            if let Some(ref items) = schema.items {
                return format!("Vec<{}>", schema_ref_to_rust(items));
            }
            return "Vec<serde_json::Value>".to_string();
        }
        return openapi_type_to_rust(ty, schema.format.as_deref());
    }
    "serde_json::Value".to_string()
}

fn find_body_type_v2(op: &SwaggerOperation) -> Option<String> {
    for param in &op.parameters {
        if param.location == "body" {
            if let Some(ref schema) = param.schema {
                return Some(schema_ref_to_rust(schema));
            }
        }
    }
    None
}

fn find_response_type_v2(op: &SwaggerOperation) -> String {
    if let Some(resp) = op.responses.get("200").or_else(|| op.responses.get("201")) {
        if let Some(ref schema) = resp.schema {
            return schema_ref_to_rust(schema);
        }
    }
    "serde_json::Value".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SWAGGER_SPEC: &str = r##"{
  "swagger": "2.0",
  "info": { "title": "Pet Store", "version": "1.0" },
  "paths": {
    "/pets": {
      "get": {
        "responses": {
          "200": { "schema": { "type": "array", "items": { "$ref": "#/definitions/Pet" } } }
        }
      },
      "post": {
        "parameters": [
          { "in": "body", "name": "body", "schema": { "$ref": "#/definitions/Pet" } }
        ],
        "responses": {
          "201": { "schema": { "$ref": "#/definitions/Pet" } }
        }
      }
    },
    "/pets/{id}": {
      "get": {
        "responses": {
          "200": { "schema": { "$ref": "#/definitions/Pet" } }
        }
      },
      "delete": {
        "responses": { "204": {} }
      }
    }
  },
  "definitions": {
    "Pet": {
      "type": "object",
      "properties": {
        "id": { "type": "integer", "format": "int64" },
        "name": { "type": "string" },
        "tag": { "type": "string" }
      }
    }
  }
}"##;

    #[test]
    fn parses_swagger_and_generates_structs() {
        let output = swagger_to_typeway(SWAGGER_SPEC).unwrap();
        assert!(output.contains("pub struct Pet {"), "got:\n{output}");
        assert!(output.contains("pub id: i64,"));
        assert!(output.contains("pub name: String,"));
    }

    #[test]
    fn generates_api_type() {
        let output = swagger_to_typeway(SWAGGER_SPEC).unwrap();
        assert!(output.contains("type API = ("));
        assert!(output.contains("GetEndpoint"));
        assert!(output.contains("PostEndpoint"));
        assert!(output.contains("DeleteEndpoint"));
    }

    #[test]
    fn generates_path_declarations() {
        let output = swagger_to_typeway(SWAGGER_SPEC).unwrap();
        assert!(output.contains("typeway_path!"));
    }

    #[test]
    fn rejects_non_swagger2() {
        let spec = r#"{"swagger": "3.0", "info": {}, "paths": {}}"#;
        let result = swagger_to_typeway(spec);
        assert!(result.is_err());
    }
}
