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
