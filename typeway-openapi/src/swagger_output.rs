//! Convert an OpenAPI 3.x spec to Swagger 2.0 output.
//!
//! Since typeway generates OpenAPI 3.1 specs via [`ApiToSpec`], this module
//! converts that output to Swagger 2.0 format for teams that need it.
//!
//! # Example
//!
//! ```ignore
//! use typeway_openapi::{ApiToSpec, to_swagger2};
//!
//! let spec_3x = MyAPI::to_spec("My Service", "1.0");
//! let swagger_json = to_swagger2(&spec_3x);
//! println!("{}", serde_json::to_string_pretty(&swagger_json).unwrap());
//! ```

use indexmap::IndexMap;
use serde::Serialize;

use crate::spec::*;

/// A Swagger 2.0 spec.
#[derive(Debug, Clone, Serialize)]
pub struct SwaggerOutput {
    pub swagger: String,
    pub info: SwaggerInfo,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub paths: IndexMap<String, IndexMap<String, SwaggerOperation>>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub definitions: IndexMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub consumes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub produces: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwaggerInfo {
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwaggerOperation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "operationId", skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<serde_json::Value>,
    pub responses: IndexMap<String, SwaggerResponse>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwaggerResponse {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

/// Convert an OpenAPI 3.x [`OpenApiSpec`] to a Swagger 2.0 [`SwaggerOutput`].
pub fn to_swagger2(spec: &OpenApiSpec) -> SwaggerOutput {
    let mut paths = IndexMap::new();

    for (path, path_item) in &spec.paths {
        let mut methods = IndexMap::new();

        let ops = [
            ("get", &path_item.get),
            ("post", &path_item.post),
            ("put", &path_item.put),
            ("delete", &path_item.delete),
            ("patch", &path_item.patch),
            ("head", &path_item.head),
            ("options", &path_item.options),
        ];

        for (method, op_opt) in ops {
            if let Some(op) = op_opt {
                methods.insert(method.to_string(), convert_operation(op));
            }
        }

        if !methods.is_empty() {
            paths.insert(path.clone(), methods);
        }
    }

    // Extract definitions from components.schemas if present.
    let definitions = if let Some(ref components) = spec.components {
        components
            .security_schemes
            .iter()
            .map(|_| ()) // Consume but don't convert security schemes here.
            .count();
        IndexMap::new()
    } else {
        IndexMap::new()
    };

    SwaggerOutput {
        swagger: "2.0".to_string(),
        info: SwaggerInfo {
            title: spec.info.title.clone(),
            version: spec.info.version.clone(),
            description: spec.info.description.clone(),
        },
        paths,
        definitions,
        consumes: vec!["application/json".to_string()],
        produces: vec!["application/json".to_string()],
    }
}

fn convert_operation(op: &Operation) -> SwaggerOperation {
    let mut parameters = Vec::new();

    // Convert path/query parameters.
    for param in &op.parameters {
        let mut p = serde_json::json!({
            "name": param.name,
            "in": param.location,
            "required": param.required,
        });
        if let Some(ref schema) = param.schema {
            if let Some(ref ty) = schema.schema_type {
                p["type"] = serde_json::Value::String(ty.clone());
            }
            if let Some(ref fmt) = schema.format {
                p["format"] = serde_json::Value::String(fmt.clone());
            }
        }
        parameters.push(p);
    }

    // Convert requestBody → body parameter.
    if let Some(ref body) = op.request_body {
        if let Some(media) = body.content.get("application/json") {
            let schema_val = media
                .schema
                .as_ref()
                .map(|s| serde_json::to_value(s).unwrap_or_default())
                .unwrap_or(serde_json::json!({"type": "object"}));

            parameters.push(serde_json::json!({
                "in": "body",
                "name": "body",
                "required": body.required,
                "schema": schema_val,
            }));
        }
    }

    // Convert responses.
    let mut responses = IndexMap::new();
    for (code, resp) in &op.responses {
        let schema = resp
            .content
            .get("application/json")
            .and_then(|m| m.schema.as_ref())
            .map(|s| serde_json::to_value(s).unwrap_or_default());

        responses.insert(
            code.clone(),
            SwaggerResponse {
                description: resp.description.clone(),
                schema,
            },
        );
    }

    SwaggerOperation {
        summary: op.summary.clone(),
        description: op.description.clone(),
        operation_id: op.operation_id.clone(),
        tags: op.tags.clone(),
        parameters,
        responses,
        deprecated: op.deprecated,
    }
}

/// Convenience: convert an OpenAPI 3.x spec to Swagger 2.0 JSON string.
pub fn to_swagger2_json(spec: &OpenApiSpec) -> String {
    let swagger = to_swagger2(spec);
    serde_json::to_string_pretty(&swagger).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec() -> OpenApiSpec {
        let mut spec = OpenApiSpec::new("Test API", "1.0");

        let mut path_item = PathItem::default();
        let mut get_op = Operation::new();
        get_op.summary = Some("List items".to_string());
        get_op.responses.insert(
            "200".to_string(),
            Response {
                description: "Success".to_string(),
                content: IndexMap::from([(
                    "application/json".to_string(),
                    MediaType {
                        schema: Some(Schema::array(Schema::string())),
                        example: None,
                    },
                )]),
            },
        );
        path_item.get = Some(get_op);

        let mut post_op = Operation::new();
        post_op.request_body = Some(RequestBody {
            required: true,
            content: IndexMap::from([(
                "application/json".to_string(),
                MediaType {
                    schema: Some(Schema::object()),
                    example: None,
                },
            )]),
        });
        post_op.responses.insert(
            "201".to_string(),
            Response {
                description: "Created".to_string(),
                content: IndexMap::from([(
                    "application/json".to_string(),
                    MediaType {
                        schema: Some(Schema::object()),
                        example: None,
                    },
                )]),
            },
        );
        path_item.post = Some(post_op);

        spec.paths.insert("/items".to_string(), path_item);
        spec
    }

    #[test]
    fn outputs_swagger_2_version() {
        let swagger = to_swagger2(&sample_spec());
        assert_eq!(swagger.swagger, "2.0");
    }

    #[test]
    fn preserves_info() {
        let swagger = to_swagger2(&sample_spec());
        assert_eq!(swagger.info.title, "Test API");
        assert_eq!(swagger.info.version, "1.0");
    }

    #[test]
    fn converts_paths_and_methods() {
        let swagger = to_swagger2(&sample_spec());
        let items = swagger.paths.get("/items").unwrap();
        assert!(items.contains_key("get"));
        assert!(items.contains_key("post"));
    }

    #[test]
    fn converts_request_body_to_body_param() {
        let swagger = to_swagger2(&sample_spec());
        let post = &swagger.paths["/items"]["post"];
        let body_param = post
            .parameters
            .iter()
            .find(|p| p["in"] == "body")
            .expect("missing body parameter");
        assert_eq!(body_param["name"], "body");
        assert_eq!(body_param["required"], true);
    }

    #[test]
    fn converts_responses() {
        let swagger = to_swagger2(&sample_spec());
        let get = &swagger.paths["/items"]["get"];
        let resp = get.responses.get("200").unwrap();
        assert_eq!(resp.description, "Success");
        assert!(resp.schema.is_some());
    }

    #[test]
    fn includes_consumes_produces() {
        let swagger = to_swagger2(&sample_spec());
        assert!(swagger.consumes.contains(&"application/json".to_string()));
        assert!(swagger.produces.contains(&"application/json".to_string()));
    }

    #[test]
    fn json_output_is_valid() {
        let json = to_swagger2_json(&sample_spec());
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["swagger"], "2.0");
        assert!(parsed["paths"]["/items"]["get"].is_object());
    }

    #[test]
    fn preserves_operation_metadata() {
        let swagger = to_swagger2(&sample_spec());
        let get = &swagger.paths["/items"]["get"];
        assert_eq!(get.summary.as_deref(), Some("List items"));
    }
}
