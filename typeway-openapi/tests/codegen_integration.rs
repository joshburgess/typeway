//! Integration tests for OpenAPI → Rust codegen (both v2 and v3).

// ===========================================================================
// codegen_common tests
// ===========================================================================

mod common {
    use typeway_openapi::codegen_common::*;

    #[test]
    fn type_mapping_string_variants() {
        assert_eq!(openapi_type_to_rust("string", None), "String");
        assert_eq!(openapi_type_to_rust("string", Some("date-time")), "String");
        assert_eq!(openapi_type_to_rust("string", Some("uuid")), "String");
        assert_eq!(openapi_type_to_rust("string", Some("binary")), "Vec<u8>");
        assert_eq!(openapi_type_to_rust("string", Some("byte")), "Vec<u8>");
    }

    #[test]
    fn type_mapping_integer_variants() {
        assert_eq!(openapi_type_to_rust("integer", None), "i64");
        assert_eq!(openapi_type_to_rust("integer", Some("int32")), "i32");
        assert_eq!(openapi_type_to_rust("integer", Some("int64")), "i64");
    }

    #[test]
    fn type_mapping_number_variants() {
        assert_eq!(openapi_type_to_rust("number", None), "f64");
        assert_eq!(openapi_type_to_rust("number", Some("float")), "f32");
        assert_eq!(openapi_type_to_rust("number", Some("double")), "f64");
    }

    #[test]
    fn type_mapping_other() {
        assert_eq!(openapi_type_to_rust("boolean", None), "bool");
        assert_eq!(openapi_type_to_rust("array", None), "Vec<serde_json::Value>");
        assert_eq!(openapi_type_to_rust("object", None), "serde_json::Value");
    }

    #[test]
    fn path_to_macro_args_conversion() {
        assert_eq!(path_to_macro_args("/users"), "\"users\"");
        assert_eq!(path_to_macro_args("/users/{id}"), "\"users\" / String");
        assert_eq!(
            path_to_macro_args("/users/{id}/posts/{postId}"),
            "\"users\" / String / \"posts\" / String"
        );
        assert_eq!(path_to_macro_args("/"), "\"\"");
    }

    #[test]
    fn path_to_type_name_conversion() {
        assert_eq!(path_to_type_name("/users"), "UsersPath");
        assert_eq!(path_to_type_name("/users/{id}"), "UsersByIdPath");
        assert_eq!(
            path_to_type_name("/users/{id}/posts"),
            "UsersByIdPostsPath"
        );
        assert_eq!(path_to_type_name("/"), "RootPath");
    }

    #[test]
    fn ref_to_name_extraction() {
        assert_eq!(ref_to_name("#/definitions/User"), "User");
        assert_eq!(ref_to_name("#/components/schemas/Pet"), "Pet");
        assert_eq!(ref_to_name("User"), "User");
    }

    #[test]
    fn endpoint_type_generation() {
        assert_eq!(
            endpoint_type("get", "UsersPath", None, "Vec<User>"),
            "GetEndpoint<UsersPath, Json<Vec<User>>>"
        );
        assert_eq!(
            endpoint_type("post", "UsersPath", Some("CreateUser"), "User"),
            "PostEndpoint<UsersPath, Json<CreateUser>, Json<User>>"
        );
        assert_eq!(
            endpoint_type("delete", "UserPath", None, "()"),
            "DeleteEndpoint<UserPath, Json<()>>"
        );
        assert_eq!(
            endpoint_type("put", "UserPath", Some("UpdateUser"), "User"),
            "PutEndpoint<UserPath, Json<UpdateUser>, Json<User>>"
        );
        assert_eq!(
            endpoint_type("patch", "UserPath", Some("PatchUser"), "User"),
            "PatchEndpoint<UserPath, Json<PatchUser>, Json<User>>"
        );
    }

    #[test]
    fn sanitize_name_replaces_special_chars() {
        assert_eq!(sanitize_name("my.type"), "my_type");
        assert_eq!(sanitize_name("my-type"), "my_type");
        assert_eq!(sanitize_name("my type"), "my_type");
        assert_eq!(sanitize_name("Simple"), "Simple");
    }
}

// ===========================================================================
// Swagger 2.x edge cases
// ===========================================================================

mod swagger {
    use typeway_openapi::swagger_to_typeway;

    #[test]
    fn handles_array_response() {
        let spec = r##"{
  "swagger": "2.0",
  "info": { "title": "Test", "version": "1.0" },
  "paths": {
    "/items": {
      "get": {
        "responses": {
          "200": {
            "schema": { "type": "array", "items": { "$ref": "#/definitions/Item" } }
          }
        }
      }
    }
  },
  "definitions": {
    "Item": {
      "type": "object",
      "properties": { "name": { "type": "string" } }
    }
  }
}"##;
        let output = swagger_to_typeway(spec).unwrap();
        assert!(output.contains("Vec<Item>"), "Expected Vec<Item>, got:\n{output}");
    }

    #[test]
    fn handles_multiple_methods_same_path() {
        let spec = r##"{
  "swagger": "2.0",
  "info": { "title": "Test", "version": "1.0" },
  "paths": {
    "/things": {
      "get": { "responses": { "200": { "schema": { "type": "string" } } } },
      "post": {
        "parameters": [{ "in": "body", "name": "body", "schema": { "type": "object" } }],
        "responses": { "201": { "schema": { "type": "string" } } }
      },
      "delete": { "responses": { "204": {} } }
    }
  },
  "definitions": {}
}"##;
        let output = swagger_to_typeway(spec).unwrap();
        assert!(output.contains("GetEndpoint"));
        assert!(output.contains("PostEndpoint"));
        assert!(output.contains("DeleteEndpoint"));
        // Only one path declaration despite 3 methods.
        let count = output.matches("typeway_path!").count();
        assert_eq!(count, 1, "Expected 1 path decl, got {count}");
    }

    #[test]
    fn handles_no_definitions() {
        let spec = r##"{
  "swagger": "2.0",
  "info": { "title": "Empty", "version": "1.0" },
  "paths": {
    "/health": {
      "get": { "responses": { "200": { "schema": { "type": "string" } } } }
    }
  }
}"##;
        let output = swagger_to_typeway(spec).unwrap();
        assert!(output.contains("type API = ("));
        assert!(output.contains("HealthPath"));
    }

    #[test]
    fn handles_path_parameters() {
        let spec = r##"{
  "swagger": "2.0",
  "info": { "title": "Test", "version": "1.0" },
  "paths": {
    "/users/{userId}/posts/{postId}": {
      "get": { "responses": { "200": { "schema": { "type": "string" } } } }
    }
  },
  "definitions": {}
}"##;
        let output = swagger_to_typeway(spec).unwrap();
        assert!(output.contains("\"users\" / String / \"posts\" / String"));
    }

    #[test]
    fn handles_yaml_input() {
        let yaml = r#"
swagger: "2.0"
info:
  title: YAML Test
  version: "1.0"
paths:
  /ping:
    get:
      responses:
        "200":
          schema:
            type: string
definitions: {}
"#;
        let output = swagger_to_typeway(yaml).unwrap();
        assert!(output.contains("PingPath"));
        assert!(output.contains("type API = ("));
    }
}

// ===========================================================================
// OpenAPI 3.x edge cases
// ===========================================================================

mod openapi3 {
    use typeway_openapi::openapi3_to_typeway;

    #[test]
    fn handles_all_http_methods() {
        let spec = r##"{
  "openapi": "3.0.0",
  "info": { "title": "Test", "version": "1.0" },
  "paths": {
    "/resource": {
      "get": { "responses": { "200": { "content": { "application/json": { "schema": { "type": "string" } } } } } },
      "post": {
        "requestBody": { "content": { "application/json": { "schema": { "type": "object" } } } },
        "responses": { "201": { "content": { "application/json": { "schema": { "type": "string" } } } } }
      },
      "put": {
        "requestBody": { "content": { "application/json": { "schema": { "type": "object" } } } },
        "responses": { "200": { "content": { "application/json": { "schema": { "type": "string" } } } } }
      },
      "patch": {
        "requestBody": { "content": { "application/json": { "schema": { "type": "object" } } } },
        "responses": { "200": { "content": { "application/json": { "schema": { "type": "string" } } } } }
      },
      "delete": { "responses": { "200": { "content": { "application/json": { "schema": { "type": "string" } } } } } }
    }
  }
}"##;
        let output = openapi3_to_typeway(spec).unwrap();
        assert!(output.contains("GetEndpoint"), "missing GET:\n{output}");
        assert!(output.contains("PostEndpoint"), "missing POST");
        assert!(output.contains("PutEndpoint"), "missing PUT");
        assert!(output.contains("PatchEndpoint"), "missing PATCH");
        assert!(output.contains("DeleteEndpoint"), "missing DELETE");
    }

    #[test]
    fn handles_no_response_body() {
        let spec = r##"{
  "openapi": "3.0.0",
  "info": { "title": "Test", "version": "1.0" },
  "paths": {
    "/fire": {
      "post": {
        "requestBody": { "content": { "application/json": { "schema": { "type": "string" } } } },
        "responses": { "204": { "description": "No content" } }
      }
    }
  }
}"##;
        let output = openapi3_to_typeway(spec).unwrap();
        // No 200/201 response → falls back to serde_json::Value.
        assert!(output.contains("serde_json::Value"));
    }

    #[test]
    fn handles_array_of_refs() {
        let spec = r##"{
  "openapi": "3.0.0",
  "info": { "title": "Test", "version": "1.0" },
  "paths": {
    "/tags": {
      "get": {
        "responses": {
          "200": { "content": { "application/json": {
            "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Tag" } }
          } } }
        }
      }
    }
  },
  "components": {
    "schemas": {
      "Tag": {
        "type": "object",
        "properties": { "name": { "type": "string" } }
      }
    }
  }
}"##;
        let output = openapi3_to_typeway(spec).unwrap();
        assert!(output.contains("Vec<Tag>"), "Expected Vec<Tag>:\n{output}");
        assert!(output.contains("pub struct Tag {"));
    }

    #[test]
    fn handles_nested_path_params() {
        let spec = r##"{
  "openapi": "3.0.0",
  "info": { "title": "Test", "version": "1.0" },
  "paths": {
    "/orgs/{orgId}/teams/{teamId}/members": {
      "get": {
        "responses": { "200": { "content": { "application/json": { "schema": { "type": "array", "items": { "type": "string" } } } } } }
      }
    }
  }
}"##;
        let output = openapi3_to_typeway(spec).unwrap();
        assert!(output.contains("\"orgs\" / String / \"teams\" / String / \"members\""));
    }

    #[test]
    fn handles_all_of_ref() {
        let spec = r##"{
  "openapi": "3.0.0",
  "info": { "title": "Test", "version": "1.0" },
  "paths": {
    "/extended": {
      "get": {
        "responses": {
          "200": { "content": { "application/json": {
            "schema": { "allOf": [{ "$ref": "#/components/schemas/Base" }] }
          } } }
        }
      }
    }
  },
  "components": {
    "schemas": {
      "Base": {
        "type": "object",
        "properties": { "id": { "type": "integer" } }
      }
    }
  }
}"##;
        let output = openapi3_to_typeway(spec).unwrap();
        // allOf with single ref resolves to Base.
        assert!(output.contains("Base"), "Expected Base ref:\n{output}");
    }

    #[test]
    fn empty_paths_generates_empty_api() {
        let spec = r##"{
  "openapi": "3.0.0",
  "info": { "title": "Empty", "version": "1.0" },
  "paths": {},
  "components": {
    "schemas": {
      "Ping": {
        "type": "object",
        "properties": { "ok": { "type": "boolean" } }
      }
    }
  }
}"##;
        let output = openapi3_to_typeway(spec).unwrap();
        // Struct generated but no API type (no paths).
        assert!(output.contains("pub struct Ping {"));
        assert!(!output.contains("type API = ("));
    }

    #[test]
    fn integer_formats_map_correctly() {
        let spec = r##"{
  "openapi": "3.0.0",
  "info": { "title": "Test", "version": "1.0" },
  "paths": {},
  "components": {
    "schemas": {
      "Metrics": {
        "type": "object",
        "required": ["count32", "count64", "score"],
        "properties": {
          "count32": { "type": "integer", "format": "int32" },
          "count64": { "type": "integer", "format": "int64" },
          "score": { "type": "number", "format": "float" }
        }
      }
    }
  }
}"##;
        let output = openapi3_to_typeway(spec).unwrap();
        assert!(output.contains("pub count32: i32,"), "got:\n{output}");
        assert!(output.contains("pub count64: i64,"));
        assert!(output.contains("pub score: f32,"));
    }
}

// ===========================================================================
// Round-trip tests: Swagger 2.0 ↔ OpenAPI 3.x conversion
// ===========================================================================

mod roundtrip {
    use typeway_openapi::{openapi3_to_typeway, swagger_to_typeway, to_swagger2, to_swagger2_json};
    use typeway_openapi::spec::*;
    use indexmap::IndexMap;

    /// Build a sample OpenAPI 3.x spec programmatically.
    fn sample_spec() -> OpenApiSpec {
        let mut spec = OpenApiSpec::new("Round-Trip Test", "2.0");

        // GET /users → array of User
        let mut users_path = PathItem::default();
        let mut get_op = Operation::new();
        get_op.summary = Some("List users".to_string());
        get_op.tags = vec!["users".to_string()];
        get_op.parameters.push(Parameter {
            name: "limit".to_string(),
            location: ParameterLocation::Query,
            required: false,
            schema: Some(Schema::integer()),
        });
        get_op.responses.insert("200".to_string(), Response {
            description: "Success".to_string(),
            content: IndexMap::from([("application/json".to_string(), MediaType {
                schema: Some(Schema::array(Schema::string())),
                example: None,
            })]),
        });
        users_path.get = Some(get_op);

        // POST /users → create user
        let mut post_op = Operation::new();
        post_op.summary = Some("Create user".to_string());
        post_op.request_body = Some(RequestBody {
            required: true,
            content: IndexMap::from([("application/json".to_string(), MediaType {
                schema: Some(Schema::object()),
                example: None,
            })]),
        });
        post_op.responses.insert("201".to_string(), Response {
            description: "Created".to_string(),
            content: IndexMap::from([("application/json".to_string(), MediaType {
                schema: Some(Schema::object()),
                example: None,
            })]),
        });
        users_path.post = Some(post_op);

        spec.paths.insert("/users".to_string(), users_path);

        // DELETE /users/{id}
        let mut user_path = PathItem::default();
        let mut delete_op = Operation::new();
        delete_op.parameters.push(Parameter {
            name: "id".to_string(),
            location: ParameterLocation::Path,
            required: true,
            schema: Some(Schema::string()),
        });
        delete_op.responses.insert("204".to_string(), Response {
            description: "Deleted".to_string(),
            content: IndexMap::new(),
        });
        user_path.delete = Some(delete_op);
        spec.paths.insert("/users/{id}".to_string(), user_path);

        spec
    }

    #[test]
    fn openapi3_to_swagger2_preserves_paths() {
        let spec = sample_spec();
        let swagger = to_swagger2(&spec);

        assert_eq!(swagger.swagger, "2.0");
        assert!(swagger.paths.contains_key("/users"));
        assert!(swagger.paths.contains_key("/users/{id}"));
        assert!(swagger.paths["/users"].contains_key("get"));
        assert!(swagger.paths["/users"].contains_key("post"));
        assert!(swagger.paths["/users/{id}"].contains_key("delete"));
    }

    #[test]
    fn openapi3_to_swagger2_converts_request_body() {
        let spec = sample_spec();
        let swagger = to_swagger2(&spec);

        let post = &swagger.paths["/users"]["post"];
        let body_param = post.parameters.iter()
            .find(|p| p["in"] == "body")
            .expect("POST should have body parameter");
        assert_eq!(body_param["required"], true);
    }

    #[test]
    fn openapi3_to_swagger2_preserves_query_params() {
        let spec = sample_spec();
        let swagger = to_swagger2(&spec);

        let get = &swagger.paths["/users"]["get"];
        let query_param = get.parameters.iter()
            .find(|p| p["in"] == "query")
            .expect("GET should have query parameter");
        assert_eq!(query_param["name"], "limit");
    }

    #[test]
    fn openapi3_to_swagger2_preserves_summary_and_tags() {
        let spec = sample_spec();
        let swagger = to_swagger2(&spec);

        let get = &swagger.paths["/users"]["get"];
        assert_eq!(get.summary.as_deref(), Some("List users"));
        assert_eq!(get.tags, vec!["users"]);
    }

    #[test]
    fn swagger2_json_roundtrip_is_parseable() {
        // Generate OpenAPI 3.x → convert to Swagger 2.0 JSON → parse back.
        let spec = sample_spec();
        let swagger_json = to_swagger2_json(&spec);

        // The Swagger 2.0 JSON should be parseable by swagger_to_typeway.
        let rust_code = swagger_to_typeway(&swagger_json).unwrap();
        assert!(rust_code.contains("type API = ("), "got:\n{rust_code}");
        assert!(rust_code.contains("GetEndpoint"));
        assert!(rust_code.contains("PostEndpoint"));
        assert!(rust_code.contains("DeleteEndpoint"));
        assert!(rust_code.contains("UsersPath"));
    }

    #[test]
    fn openapi3_to_swagger2_to_rust_produces_valid_code() {
        // Full pipeline: OpenAPI 3.x spec → Swagger 2.0 → Rust codegen.
        let spec = sample_spec();
        let swagger_json = to_swagger2_json(&spec);
        let rust_code = swagger_to_typeway(&swagger_json).unwrap();

        // Should have path declarations.
        assert!(rust_code.contains("typeway_path!"));
        // Should have use statements.
        assert!(rust_code.contains("use typeway::prelude::*;"));
        assert!(rust_code.contains("use serde::{Serialize, Deserialize};"));
    }

    #[test]
    fn openapi3_json_roundtrip_is_parseable() {
        // Generate OpenAPI 3.x spec → serialize to JSON → parse back.
        let spec = sample_spec();
        let json = serde_json::to_string_pretty(&spec).unwrap();

        let rust_code = openapi3_to_typeway(&json).unwrap();
        assert!(rust_code.contains("type API = ("), "got:\n{rust_code}");
        assert!(rust_code.contains("GetEndpoint"));
        assert!(rust_code.contains("PostEndpoint"));
    }
}
