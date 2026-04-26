use typeway_migrate::parse::axum::parse_axum_file;

const FIXTURE: &str = include_str!("fixtures/simple_axum.rs");

#[test]
fn parses_simple_axum_routes() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    assert_eq!(model.endpoints.len(), 4, "should find 4 endpoints");

    let methods: Vec<_> = model
        .endpoints
        .iter()
        .map(|ep| format!("{:?} {}", ep.method, ep.path.raw_pattern))
        .collect();

    assert!(methods.contains(&"Get /users".to_string()));
    assert!(methods.contains(&"Post /users".to_string()));
    assert!(methods.contains(&"Get /users/{id}".to_string()));
    assert!(methods.contains(&"Delete /users/{id}".to_string()));
}

#[test]
fn extracts_handler_names() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let names: Vec<_> = model
        .endpoints
        .iter()
        .map(|ep| ep.handler.name.to_string())
        .collect();

    assert!(names.contains(&"list_users".to_string()));
    assert!(names.contains(&"get_user".to_string()));
    assert!(names.contains(&"create_user".to_string()));
    assert!(names.contains(&"delete_user".to_string()));
}

#[test]
fn generates_path_type_names() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let path_names: Vec<_> = model
        .endpoints
        .iter()
        .map(|ep| ep.path.typeway_type_name.to_string())
        .collect();

    // /users → UsersPath
    assert!(path_names.contains(&"UsersPath".to_string()));
    // /users/{id} → UsersByIdPath
    assert!(path_names.contains(&"UsersByIdPath".to_string()));
}

#[test]
fn detects_request_body_type() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let create = model
        .endpoints
        .iter()
        .find(|ep| ep.handler.name == "create_user")
        .expect("should find create_user");

    assert!(
        create.request_body.is_some(),
        "create_user should have a request body"
    );
}

#[test]
fn full_conversion_produces_valid_rust() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    // Verify the output contains key typeway constructs.
    // prettyplease formats macros with spaces, so check without `!` directly.
    assert!(
        output.contains("typeway_path"),
        "should contain typeway_path! declarations, got:\n{output}"
    );
    assert!(
        output.contains("type API"),
        "should contain API type declaration, got:\n{output}"
    );
    assert!(
        output.contains("Server"),
        "should contain Server construction, got:\n{output}"
    );
    assert!(
        output.contains("bind"),
        "should contain bind! macros, got:\n{output}"
    );

    // The output should parse as valid Rust syntax.
    let parsed: Result<syn::File, _> = syn::parse_str(&output);
    assert!(
        parsed.is_ok(),
        "output should be valid Rust syntax: {:?}",
        parsed.err()
    );
}

#[test]
fn handler_extractors_are_transformed() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    // Path extractors should use the generated path type, not raw u32.
    assert!(
        output.contains("UsersByIdPath"),
        "Path extractor should reference generated path type, got:\n{output}"
    );

    // Destructuring should be moved to let binding.
    // prettyplease may format as "path.0" or "path . 0".
    assert!(
        output.contains("path") && output.contains(".0"),
        "should contain path.0 destructuring, got:\n{output}"
    );
}
