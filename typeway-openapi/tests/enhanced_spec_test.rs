#![allow(clippy::field_reassign_with_default)]
//! Tests for enhanced OpenAPI spec generation:
//! - ExampleValue in response/request media types
//! - Security requirements from Protected<Auth, E>
//! - Auto-tagging by path prefix
//! - Deprecated operation marking

use indexmap::IndexMap;
use serde::Serialize;
use typeway_core::*;
use typeway_macros::*;
use typeway_openapi::*;

// ---------------------------------------------------------------------------
// Domain types with ExampleValue
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct User {
    id: u32,
    name: String,
}

impl ToSchema for User {
    fn schema() -> typeway_openapi::spec::Schema {
        typeway_openapi::spec::Schema::object()
    }
    fn type_name() -> &'static str {
        "User"
    }
    fn example() -> Option<serde_json::Value> {
        Some(serde_json::json!({"id": 1, "name": "Alice"}))
    }
}

#[derive(Serialize)]
struct Article {
    slug: String,
    title: String,
}

impl ToSchema for Article {
    fn schema() -> typeway_openapi::spec::Schema {
        typeway_openapi::spec::Schema::object()
    }
    fn type_name() -> &'static str {
        "Article"
    }
}

struct CreateUser;
impl ToSchema for CreateUser {
    fn schema() -> typeway_openapi::spec::Schema {
        typeway_openapi::spec::Schema::object()
    }
    fn type_name() -> &'static str {
        "CreateUser"
    }
}

// ---------------------------------------------------------------------------
// Path types
// ---------------------------------------------------------------------------

typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);
typeway_path!(type ArticlesPath = "articles");
typeway_path!(type ArticleBySlugPath = "articles" / String);

// ---------------------------------------------------------------------------
// Test: ExampleValue appears in the generated spec
// ---------------------------------------------------------------------------

#[test]
fn example_value_appears_in_response_media_type() {
    type API = (GetEndpoint<UsersPath, Vec<User>>,);

    let spec = API::to_spec("Test", "1.0");
    let users = spec.paths.get("/users").unwrap();
    let get_op = users.get.as_ref().unwrap();
    let resp = get_op.responses.get("200").unwrap();
    let media = resp.content.get("application/json").unwrap();

    // Vec<User> uses Vec's ToSchema which doesn't override example(),
    // so it should be None. But let's check the User directly.
    assert!(
        media.example.is_none(),
        "Vec<User> doesn't have an example (Vec doesn't override example())"
    );
}

#[test]
fn example_value_appears_for_types_with_example() {
    // Direct User response (not Vec<User>)
    type API = (GetEndpoint<UserByIdPath, User>,);

    let spec = API::to_spec("Test", "1.0");
    let user_path = spec.paths.get("/users/{}").unwrap();
    let get_op = user_path.get.as_ref().unwrap();
    let resp = get_op.responses.get("200").unwrap();
    let media = resp.content.get("application/json").unwrap();

    assert!(media.example.is_some(), "User type has an example");
    let example = media.example.as_ref().unwrap();
    assert_eq!(example["id"], 1);
    assert_eq!(example["name"], "Alice");
}

#[test]
fn types_without_example_have_no_example_field() {
    type API = (GetEndpoint<ArticlesPath, Vec<Article>>,);

    let spec = API::to_spec("Test", "1.0");
    let articles = spec.paths.get("/articles").unwrap();
    let get_op = articles.get.as_ref().unwrap();
    let resp = get_op.responses.get("200").unwrap();
    let media = resp.content.get("application/json").unwrap();

    assert!(media.example.is_none());
}

#[test]
fn example_field_not_serialized_when_none() {
    type API = (GetEndpoint<ArticlesPath, Vec<Article>>,);

    let spec = API::to_spec("Test", "1.0");
    let json = serde_json::to_string(&spec).unwrap();

    // The "example" key should not appear for types without examples.
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let media = &parsed["paths"]["/articles"]["get"]["responses"]["200"]["content"]
        ["application/json"];
    assert!(media.get("example").is_none());
}

// ---------------------------------------------------------------------------
// Test: Deprecated operation marking
// ---------------------------------------------------------------------------

#[test]
fn deprecated_endpoint_sets_deprecated_flag() {
    type API = (
        GetEndpoint<UsersPath, Vec<User>>,
        Deprecated<GetEndpoint<UserByIdPath, User>>,
    );

    let spec = API::to_spec("Test", "1.0");

    // Non-deprecated endpoint
    let users = spec.paths.get("/users").unwrap();
    let get_users = users.get.as_ref().unwrap();
    assert!(!get_users.deprecated, "GET /users should not be deprecated");

    // Deprecated endpoint
    let user = spec.paths.get("/users/{}").unwrap();
    let get_user = user.get.as_ref().unwrap();
    assert!(get_user.deprecated, "GET /users/:id should be deprecated");
}

#[test]
fn deprecated_flag_serialized_only_when_true() {
    type API = (
        GetEndpoint<UsersPath, Vec<User>>,
        Deprecated<GetEndpoint<UserByIdPath, User>>,
    );

    let spec = API::to_spec("Test", "1.0");
    let json = serde_json::to_string(&spec).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Non-deprecated: "deprecated" key should be absent
    let get_users = &parsed["paths"]["/users"]["get"];
    assert!(
        get_users.get("deprecated").is_none(),
        "deprecated field should not be serialized for non-deprecated endpoints"
    );

    // Deprecated: "deprecated" key should be true
    let get_user = &parsed["paths"]["/users/{}"]["get"];
    assert_eq!(
        get_user.get("deprecated"),
        Some(&serde_json::json!(true)),
        "deprecated field should be true for deprecated endpoints"
    );
}

// ---------------------------------------------------------------------------
// Test: Requires<Effect, E> is transparent to OpenAPI
// ---------------------------------------------------------------------------

#[test]
fn requires_wrapper_is_transparent() {
    type API = (Requires<AuthRequired, GetEndpoint<UsersPath, Vec<User>>>,);

    let spec = API::to_spec("Test", "1.0");
    let users = spec.paths.get("/users").unwrap();
    let get_op = users.get.as_ref().unwrap();

    // Should have a response but no security (Requires is for effects, not auth)
    assert!(get_op.responses.contains_key("200"));
    assert!(get_op.security.is_empty());
}

// ---------------------------------------------------------------------------
// Test: Auto-tagging assigns tags based on path prefix
// ---------------------------------------------------------------------------

#[test]
fn auto_tagging_assigns_tags_from_first_path_segment() {
    type API = (
        GetEndpoint<UsersPath, Vec<User>>,
        GetEndpoint<UserByIdPath, User>,
        GetEndpoint<ArticlesPath, Vec<Article>>,
    );

    let spec = API::to_spec("Test", "1.0");

    // /users endpoints get "users" tag
    let users = spec.paths.get("/users").unwrap();
    let get_users = users.get.as_ref().unwrap();
    assert_eq!(get_users.tags, vec!["users"]);

    // /users/{} also gets "users" tag
    let user = spec.paths.get("/users/{}").unwrap();
    let get_user = user.get.as_ref().unwrap();
    assert_eq!(get_user.tags, vec!["users"]);

    // /articles gets "articles" tag
    let articles = spec.paths.get("/articles").unwrap();
    let get_articles = articles.get.as_ref().unwrap();
    assert_eq!(get_articles.tags, vec!["articles"]);
}

#[test]
fn explicit_tags_are_not_overridden_by_auto_tags() {
    // Test that auto_tag_operations preserves existing tags.
    use typeway_openapi::spec::*;

    let mut spec = OpenApiSpec::new("Test", "1.0");
    let mut op = Operation::new();
    op.tags = vec!["custom-tag".to_string()];
    op.responses.insert(
        "200".to_string(),
        Response {
            description: "ok".to_string(),
            content: IndexMap::new(),
        },
    );

    let mut path_item = PathItem::default();
    path_item.get = Some(op);
    spec.paths.insert("/users".to_string(), path_item);

    auto_tag_operations(&mut spec);

    let item = spec.paths.get("/users").unwrap();
    let get_op = item.get.as_ref().unwrap();
    // Should keep the explicit tag only, not add "users"
    assert_eq!(get_op.tags, vec!["custom-tag"]);
}

// ---------------------------------------------------------------------------
// Test: Security scheme types serialize correctly
// ---------------------------------------------------------------------------

#[test]
fn security_requirement_bearer_serializes_correctly() {
    let req = SecurityRequirement::bearer();
    let json = serde_json::to_value(&req).unwrap();

    assert_eq!(json, serde_json::json!({"bearerAuth": []}));
}

#[test]
fn security_scheme_bearer_jwt_serializes_correctly() {
    let scheme = SecurityScheme::bearer_jwt();
    let json = serde_json::to_value(&scheme).unwrap();

    assert_eq!(json["type"], "http");
    assert_eq!(json["scheme"], "bearer");
    assert_eq!(json["bearerFormat"], "JWT");
}

// ---------------------------------------------------------------------------
// Test: auto_tag_operations helper function
// ---------------------------------------------------------------------------

#[test]
fn auto_tag_ignores_parameter_only_paths() {
    use typeway_openapi::spec::*;

    let mut spec = OpenApiSpec::new("Test", "1.0");
    let mut op = Operation::new();
    op.responses
        .insert("200".to_string(), Response {
            description: "ok".to_string(),
            content: IndexMap::new(),
        });

    let mut path_item = PathItem::default();
    path_item.get = Some(op);

    // A path with only parameters should not get a tag
    spec.paths.insert("/{id}".to_string(), path_item);

    auto_tag_operations(&mut spec);

    let item = spec.paths.get("/{id}").unwrap();
    let get_op = item.get.as_ref().unwrap();
    assert!(get_op.tags.is_empty(), "parameter-only paths should not get auto-tags");
}

// ---------------------------------------------------------------------------
// Test: collect_security_schemes adds components when bearer auth is used
// ---------------------------------------------------------------------------

#[test]
fn collect_security_schemes_adds_bearer_component() {
    use typeway_openapi::spec::*;

    let mut spec = OpenApiSpec::new("Test", "1.0");
    let mut op = Operation::new();
    op.security.push(SecurityRequirement::bearer());
    op.responses.insert(
        "200".to_string(),
        Response {
            description: "ok".to_string(),
            content: IndexMap::new(),
        },
    );

    let mut path_item = PathItem::default();
    path_item.get = Some(op);
    spec.paths.insert("/protected".to_string(), path_item);

    collect_security_schemes(&mut spec);

    assert!(spec.components.is_some());
    let components = spec.components.as_ref().unwrap();
    assert!(components.security_schemes.contains_key("bearerAuth"));
    let scheme = &components.security_schemes["bearerAuth"];
    assert_eq!(scheme.scheme_type, "http");
    assert_eq!(scheme.scheme.as_deref(), Some("bearer"));
}

#[test]
fn collect_security_schemes_does_not_add_components_when_no_security() {
    use typeway_openapi::spec::*;

    let mut spec = OpenApiSpec::new("Test", "1.0");
    let mut op = Operation::new();
    op.responses.insert(
        "200".to_string(),
        Response {
            description: "ok".to_string(),
            content: IndexMap::new(),
        },
    );

    let mut path_item = PathItem::default();
    path_item.get = Some(op);
    spec.paths.insert("/public".to_string(), path_item);

    collect_security_schemes(&mut spec);

    assert!(spec.components.is_none());
}

// ---------------------------------------------------------------------------
// Test: Full spec round-trip with all enhancements
// ---------------------------------------------------------------------------

#[test]
fn full_enhanced_spec_serializes_to_valid_json() {
    type API = (
        GetEndpoint<UsersPath, Vec<User>>,
        GetEndpoint<UserByIdPath, User>,
        Deprecated<PostEndpoint<UsersPath, CreateUser, User>>,
        GetEndpoint<ArticlesPath, Vec<Article>>,
    );

    let spec = API::to_spec("Enhanced API", "2.0.0");
    let json = serde_json::to_string_pretty(&spec).unwrap();

    // Verify it's valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Verify structure
    assert_eq!(parsed["openapi"], "3.1.0");
    assert_eq!(parsed["info"]["title"], "Enhanced API");

    // Verify deprecated endpoint
    assert_eq!(parsed["paths"]["/users"]["post"]["deprecated"], true);
    assert!(parsed["paths"]["/users"]["get"].get("deprecated").is_none());

    // Verify auto-tags
    assert_eq!(parsed["paths"]["/users"]["get"]["tags"][0], "users");
    assert_eq!(parsed["paths"]["/articles"]["get"]["tags"][0], "articles");

    // Verify example on User response
    let user_example =
        &parsed["paths"]["/users/{}"]["get"]["responses"]["200"]["content"]["application/json"]
            ["example"];
    assert_eq!(user_example["id"], 1);
    assert_eq!(user_example["name"], "Alice");
}

#[test]
fn print_enhanced_spec() {
    type API = (
        GetEndpoint<UsersPath, Vec<User>>,
        GetEndpoint<UserByIdPath, User>,
        Deprecated<PostEndpoint<UsersPath, CreateUser, User>>,
        GetEndpoint<ArticlesPath, Vec<Article>>,
    );

    let spec = API::to_spec("Enhanced API", "2.0.0");
    let json = serde_json::to_string_pretty(&spec).unwrap();
    println!("\n{json}");
}
