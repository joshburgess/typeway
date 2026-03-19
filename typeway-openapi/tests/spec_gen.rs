use typeway_core::*;
use typeway_macros::*;
use typeway_openapi::*;

// --- Schema impls for domain types ---

struct User;
impl ToSchema for User {
    fn schema() -> typeway_openapi::spec::Schema {
        typeway_openapi::spec::Schema::object()
    }
    fn type_name() -> &'static str {
        "User"
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

// --- Path types ---

typeway_path!(type UsersPath = "users");
typeway_path!(type UserByIdPath = "users" / u32);

// --- API type ---

type TestAPI = (
    GetEndpoint<UsersPath, Vec<User>>,
    GetEndpoint<UserByIdPath, User>,
    PostEndpoint<UsersPath, CreateUser, User>,
    DeleteEndpoint<UserByIdPath, ()>,
);

#[test]
fn generates_correct_paths() {
    let spec = TestAPI::to_spec("Test API", "1.0.0");

    assert_eq!(spec.openapi, "3.1.0");
    assert_eq!(spec.info.title, "Test API");
    assert_eq!(spec.info.version, "1.0.0");

    // Should have two paths: /users and /users/{}
    assert_eq!(spec.paths.len(), 2);
    assert!(spec.paths.contains_key("/users"));
    assert!(spec.paths.contains_key("/users/{}"));
}

#[test]
fn users_path_has_get_and_post() {
    let spec = TestAPI::to_spec("Test", "1.0");
    let users = spec.paths.get("/users").unwrap();

    assert!(users.get.is_some(), "GET /users should exist");
    assert!(users.post.is_some(), "POST /users should exist");
    assert!(users.put.is_none(), "PUT /users should not exist");
    assert!(users.delete.is_none(), "DELETE /users should not exist");
}

#[test]
fn user_by_id_path_has_get_and_delete() {
    let spec = TestAPI::to_spec("Test", "1.0");
    let user = spec.paths.get("/users/{}").unwrap();

    assert!(user.get.is_some(), "GET /users/:id should exist");
    assert!(user.delete.is_some(), "DELETE /users/:id should exist");
    assert!(user.post.is_none(), "POST /users/:id should not exist");
}

#[test]
fn get_user_has_path_parameter() {
    let spec = TestAPI::to_spec("Test", "1.0");
    let user_path = spec.paths.get("/users/{}").unwrap();
    let get_op = user_path.get.as_ref().unwrap();

    assert_eq!(get_op.parameters.len(), 1);
    assert_eq!(get_op.parameters[0].name, "param0");
    assert!(get_op.parameters[0].required);
    assert!(get_op.parameters[0].schema.is_some());
}

#[test]
fn post_users_has_request_body() {
    let spec = TestAPI::to_spec("Test", "1.0");
    let users = spec.paths.get("/users").unwrap();
    let post_op = users.post.as_ref().unwrap();

    assert!(post_op.request_body.is_some());
    let body = post_op.request_body.as_ref().unwrap();
    assert!(body.required);
    assert!(body.content.contains_key("application/json"));
}

#[test]
fn get_users_has_response_schema() {
    let spec = TestAPI::to_spec("Test", "1.0");
    let users = spec.paths.get("/users").unwrap();
    let get_op = users.get.as_ref().unwrap();

    assert!(get_op.responses.contains_key("200"));
    let resp = get_op.responses.get("200").unwrap();
    assert!(resp.content.contains_key("application/json"));
    let media = resp.content.get("application/json").unwrap();
    let schema = media.schema.as_ref().unwrap();
    assert_eq!(schema.schema_type.as_deref(), Some("array"));
}

#[test]
fn spec_serializes_to_valid_json() {
    let spec = TestAPI::to_spec("Test API", "1.0.0");
    let json = serde_json::to_string_pretty(&spec).unwrap();

    // Parse it back to verify it's valid JSON.
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Spot-check key fields exist.
    assert!(json.contains("\"openapi\": \"3.1.0\""));
    assert!(json.contains("\"title\": \"Test API\""));
    assert!(json.contains("\"/users\""));
    assert!(json.contains("\"application/json\""));
}

#[test]
fn print_spec() {
    let spec = TestAPI::to_spec("Users API", "1.0.0");
    let json = serde_json::to_string_pretty(&spec).unwrap();
    println!("\n{json}");
}
