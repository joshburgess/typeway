//! OpenAPI 3.x → Typeway Rust codegen.
//!
//! Parses an OpenAPI 3.0/3.1 spec and generates Rust types, paths, and an
//! API type alias compatible with the typeway framework.
//!
//! # Example
//!
//! ```ignore
//! let spec = std::fs::read_to_string("openapi.yaml").unwrap();
//! let rust_code = typeway_openapi::codegen_v3::openapi3_to_typeway(&spec).unwrap();
//! std::fs::write("src/generated.rs", rust_code).unwrap();
//! ```

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::codegen_common::*;

// ---------------------------------------------------------------------------
// OpenAPI 3.x AST (minimal subset)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct OpenApi3Spec {
    openapi: String,
    #[serde(default)]
    info: OpenApi3Info,
    #[serde(default)]
    paths: BTreeMap<String, BTreeMap<String, OpenApi3Operation>>,
    #[serde(default)]
    components: Option<OpenApi3Components>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenApi3Info {
    #[serde(default)]
    title: String,
    #[serde(default)]
    version: String,
}

#[derive(Debug, Default, Deserialize)]
struct OpenApi3Components {
    #[serde(default)]
    schemas: BTreeMap<String, OpenApi3Schema>,
}

#[derive(Debug, Deserialize)]
struct OpenApi3Operation {
    #[serde(default)]
    summary: Option<String>,
    #[serde(rename = "requestBody")]
    #[serde(default)]
    request_body: Option<OpenApi3RequestBody>,
    #[serde(default)]
    responses: BTreeMap<String, OpenApi3Response>,
}

#[derive(Debug, Deserialize)]
struct OpenApi3RequestBody {
    #[serde(default)]
    content: BTreeMap<String, OpenApi3MediaType>,
}

#[derive(Debug, Deserialize)]
struct OpenApi3Response {
    #[serde(default)]
    content: Option<BTreeMap<String, OpenApi3MediaType>>,
}

#[derive(Debug, Deserialize)]
struct OpenApi3MediaType {
    #[serde(default)]
    schema: Option<OpenApi3SchemaRef>,
}

#[derive(Debug, Deserialize)]
struct OpenApi3SchemaRef {
    #[serde(rename = "$ref")]
    ref_path: Option<String>,
    #[serde(rename = "type")]
    schema_type: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    items: Option<Box<OpenApi3SchemaRef>>,
    #[serde(default)]
    properties: BTreeMap<String, OpenApi3SchemaRef>,
    #[serde(rename = "allOf")]
    #[serde(default)]
    all_of: Vec<OpenApi3SchemaRef>,
    #[serde(rename = "oneOf")]
    #[serde(default)]
    one_of: Vec<OpenApi3SchemaRef>,
}

#[derive(Debug, Deserialize)]
struct OpenApi3Schema {
    #[serde(rename = "type", default)]
    schema_type: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    properties: BTreeMap<String, OpenApi3SchemaRef>,
    #[serde(default)]
    items: Option<Box<OpenApi3SchemaRef>>,
    #[serde(rename = "enum", default)]
    enum_values: Vec<String>,
    #[serde(default)]
    required: Vec<String>,
}

// ---------------------------------------------------------------------------
// Codegen
// ---------------------------------------------------------------------------

/// Generate typeway Rust code from an OpenAPI 3.x spec (JSON or YAML).
pub fn openapi3_to_typeway(source: &str) -> Result<String, String> {
    let spec: OpenApi3Spec = serde_json::from_str(source)
        .or_else(|_| serde_yaml::from_str(source))
        .map_err(|e| format!("failed to parse OpenAPI spec: {e}"))?;

    if !spec.openapi.starts_with('3') {
        return Err(format!(
            "expected OpenAPI 3.x, got version '{}'",
            spec.openapi
        ));
    }

    let mut output = String::new();

    // Header.
    output.push_str(&format!(
        "//! Generated from OpenAPI 3.x spec: {}\n",
        spec.info.title
    ));
    output.push_str("//! Manual edits may be overwritten.\n\n");
    output.push_str("use typeway::prelude::*;\n");
    output.push_str("use serde::{Serialize, Deserialize};\n\n");

    // Generate structs/enums from component schemas.
    if let Some(ref components) = spec.components {
        for (name, schema) in &components.schemas {
            if !schema.enum_values.is_empty() {
                output.push_str(&generate_enum_from_values(name, &schema.enum_values));
            } else {
                let fields = schema_to_fields_v3(schema);
                output.push_str(&generate_struct(name, &fields));
            }
            output.push_str("\n\n");
        }
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

            let req_type = find_body_type_v3(operation);
            let res_type = find_response_type_v3(operation);
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

fn schema_to_fields_v3(schema: &OpenApi3Schema) -> Vec<(String, String)> {
    let required: std::collections::HashSet<&str> =
        schema.required.iter().map(|s| s.as_str()).collect();

    schema
        .properties
        .iter()
        .map(|(name, prop)| {
            let base_type = schema_ref_to_rust_v3(prop);
            let rust_type = if required.contains(name.as_str()) {
                base_type
            } else {
                format!("Option<{}>", base_type)
            };
            (name.clone(), rust_type)
        })
        .collect()
}

fn schema_ref_to_rust_v3(schema: &OpenApi3SchemaRef) -> String {
    if let Some(ref ref_path) = schema.ref_path {
        return sanitize_name(&ref_to_name(ref_path));
    }
    if let Some(ref ty) = schema.schema_type {
        if ty == "array" {
            if let Some(ref items) = schema.items {
                return format!("Vec<{}>", schema_ref_to_rust_v3(items));
            }
            return "Vec<serde_json::Value>".to_string();
        }
        if ty == "object" && !schema.properties.is_empty() {
            // Inline object — use serde_json::Value for now.
            return "serde_json::Value".to_string();
        }
        return openapi_type_to_rust(ty, schema.format.as_deref());
    }
    // allOf / oneOf — take first ref.
    if let Some(first) = schema.all_of.first().or(schema.one_of.first()) {
        return schema_ref_to_rust_v3(first);
    }
    "serde_json::Value".to_string()
}

fn find_body_type_v3(op: &OpenApi3Operation) -> Option<String> {
    let body = op.request_body.as_ref()?;
    let media = body
        .content
        .get("application/json")
        .or_else(|| body.content.values().next())?;
    let schema = media.schema.as_ref()?;
    Some(schema_ref_to_rust_v3(schema))
}

fn find_response_type_v3(op: &OpenApi3Operation) -> String {
    let resp = op
        .responses
        .get("200")
        .or_else(|| op.responses.get("201"))
        .or_else(|| op.responses.get("default"));

    if let Some(resp) = resp {
        if let Some(ref content) = resp.content {
            let media = content
                .get("application/json")
                .or_else(|| content.values().next());
            if let Some(media) = media {
                if let Some(ref schema) = media.schema {
                    return schema_ref_to_rust_v3(schema);
                }
            }
        }
    }
    "serde_json::Value".to_string()
}

fn generate_enum_from_values(name: &str, values: &[String]) -> String {
    let mut s = String::new();
    s.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
    s.push_str(&format!("pub enum {} {{\n", sanitize_name(name)));
    for val in values {
        let variant = capitalize(val);
        s.push_str(&format!("    #[serde(rename = \"{}\")]\n", val));
        s.push_str(&format!("    {},\n", variant));
    }
    s.push('}');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENAPI3_SPEC: &str = r##"{
  "openapi": "3.0.3",
  "info": { "title": "Pet Store", "version": "1.0.0" },
  "paths": {
    "/pets": {
      "get": {
        "summary": "List pets",
        "responses": {
          "200": {
            "content": {
              "application/json": {
                "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Pet" } }
              }
            }
          }
        }
      },
      "post": {
        "summary": "Create pet",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": { "$ref": "#/components/schemas/CreatePet" }
            }
          }
        },
        "responses": {
          "201": {
            "content": {
              "application/json": {
                "schema": { "$ref": "#/components/schemas/Pet" }
              }
            }
          }
        }
      }
    },
    "/pets/{id}": {
      "get": {
        "responses": {
          "200": {
            "content": {
              "application/json": {
                "schema": { "$ref": "#/components/schemas/Pet" }
              }
            }
          }
        }
      }
    }
  },
  "components": {
    "schemas": {
      "Pet": {
        "type": "object",
        "required": ["id", "name"],
        "properties": {
          "id": { "type": "integer", "format": "int64" },
          "name": { "type": "string" },
          "tag": { "type": "string" },
          "status": { "$ref": "#/components/schemas/PetStatus" }
        }
      },
      "CreatePet": {
        "type": "object",
        "required": ["name"],
        "properties": {
          "name": { "type": "string" },
          "tag": { "type": "string" }
        }
      },
      "PetStatus": {
        "type": "string",
        "enum": ["available", "pending", "sold"]
      }
    }
  }
}"##;

    #[test]
    fn parses_openapi3_and_generates_structs() {
        let output = openapi3_to_typeway(OPENAPI3_SPEC).unwrap();
        assert!(output.contains("pub struct Pet {"), "got:\n{output}");
        assert!(output.contains("pub id: i64,"));
        assert!(output.contains("pub name: String,"));
        // Non-required fields are Option.
        assert!(output.contains("pub tag: Option<String>,"));
    }

    #[test]
    fn generates_enums_from_string_enums() {
        let output = openapi3_to_typeway(OPENAPI3_SPEC).unwrap();
        assert!(output.contains("pub enum PetStatus {"), "got:\n{output}");
        assert!(output.contains("Available,"));
        assert!(output.contains("Pending,"));
        assert!(output.contains("Sold,"));
    }

    #[test]
    fn generates_api_type_with_correct_methods() {
        let output = openapi3_to_typeway(OPENAPI3_SPEC).unwrap();
        assert!(output.contains("type API = ("));
        assert!(output.contains("GetEndpoint"));
        assert!(output.contains("PostEndpoint"));
    }

    #[test]
    fn generates_path_declarations() {
        let output = openapi3_to_typeway(OPENAPI3_SPEC).unwrap();
        assert!(output.contains("typeway_path!"));
        assert!(output.contains("PetsPath"));
    }

    #[test]
    fn required_fields_are_not_optional() {
        let output = openapi3_to_typeway(OPENAPI3_SPEC).unwrap();
        // id is required — no Option wrapper.
        assert!(output.contains("pub id: i64,"));
        // tag is NOT required — Option wrapper.
        assert!(output.contains("pub tag: Option<String>,"));
    }

    #[test]
    fn rejects_non_openapi3() {
        let spec = r#"{"openapi": "2.0", "info": {}, "paths": {}}"#;
        let result = openapi3_to_typeway(spec);
        assert!(result.is_err());
    }

    #[test]
    fn handles_yaml_input() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1.0"
paths:
  /health:
    get:
      responses:
        "200":
          content:
            application/json:
              schema:
                type: string
"#;
        let output = openapi3_to_typeway(yaml).unwrap();
        assert!(output.contains("type API = ("));
        assert!(output.contains("HealthPath"));
    }

    #[test]
    fn ref_resolution() {
        let output = openapi3_to_typeway(OPENAPI3_SPEC).unwrap();
        // The CreatePet ref in request body should resolve to CreatePet.
        assert!(output.contains("CreatePet"));
    }
}
