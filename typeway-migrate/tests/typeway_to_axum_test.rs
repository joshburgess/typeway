use typeway_migrate::parse::typeway::parse_typeway_file;

const FIXTURE: &str = include_str!("fixtures/simple_typeway.rs");

#[test]
fn parses_typeway_path_declarations() {
    let model = parse_typeway_file(FIXTURE).expect("should parse");

    let path_names: Vec<_> = model
        .endpoints
        .iter()
        .map(|ep| ep.path.typeway_type_name.to_string())
        .collect();

    assert!(
        path_names.contains(&"UsersPath".to_string()),
        "should find UsersPath, got: {:?}",
        path_names
    );
    assert!(
        path_names.contains(&"UsersByIdPath".to_string()),
        "should find UsersByIdPath, got: {:?}",
        path_names
    );

    // Check that UsersPath has the correct segments.
    let users_ep = model
        .endpoints
        .iter()
        .find(|ep| ep.path.typeway_type_name == "UsersPath")
        .expect("should find UsersPath endpoint");
    assert_eq!(users_ep.path.segments.len(), 1);

    // Check that UsersByIdPath has the correct segments.
    let users_by_id_ep = model
        .endpoints
        .iter()
        .find(|ep| ep.path.typeway_type_name == "UsersByIdPath")
        .expect("should find UsersByIdPath endpoint");
    assert_eq!(users_by_id_ep.path.segments.len(), 2);
}

#[test]
fn parses_api_type() {
    let model = parse_typeway_file(FIXTURE).expect("should parse");

    assert_eq!(model.endpoints.len(), 4, "should find 4 endpoints");

    let methods: Vec<_> = model
        .endpoints
        .iter()
        .map(|ep| format!("{:?}", ep.method))
        .collect();

    // Two GETs, one POST, one DELETE.
    assert_eq!(
        methods.iter().filter(|m| *m == "Get").count(),
        2,
        "should have 2 GET endpoints"
    );
    assert_eq!(
        methods.iter().filter(|m| *m == "Post").count(),
        1,
        "should have 1 POST endpoint"
    );
    assert_eq!(
        methods.iter().filter(|m| *m == "Delete").count(),
        1,
        "should have 1 DELETE endpoint"
    );
}

#[test]
fn extracts_handler_names() {
    let model = parse_typeway_file(FIXTURE).expect("should parse");

    let names: Vec<_> = model
        .endpoints
        .iter()
        .map(|ep| ep.handler.name.to_string())
        .collect();

    assert!(
        names.contains(&"list_users".to_string()),
        "should find list_users, got: {:?}",
        names
    );
    assert!(
        names.contains(&"get_user".to_string()),
        "should find get_user, got: {:?}",
        names
    );
    assert!(
        names.contains(&"create_user".to_string()),
        "should find create_user, got: {:?}",
        names
    );
    assert!(
        names.contains(&"delete_user".to_string()),
        "should find delete_user, got: {:?}",
        names
    );
}

#[test]
fn full_conversion_produces_valid_rust() {
    let output = typeway_migrate::typeway_to_axum(FIXTURE).expect("conversion should succeed");

    // The output should contain key Axum constructs.
    assert!(
        output.contains("Router::new()") || output.contains("Router :: new()"),
        "should contain Router::new(), got:\n{output}"
    );
    assert!(
        output.contains(".route(") || output.contains(". route("),
        "should contain .route( calls, got:\n{output}"
    );
    assert!(
        output.contains("get(") || output.contains("get ("),
        "should contain get( routing function, got:\n{output}"
    );
    assert!(
        output.contains("post(") || output.contains("post ("),
        "should contain post( routing function, got:\n{output}"
    );

    // The output should parse as valid Rust syntax.
    let parsed: Result<syn::File, _> = syn::parse_str(&output);
    assert!(
        parsed.is_ok(),
        "output should be valid Rust syntax: {:?}\n\nOutput was:\n{}",
        parsed.err(),
        output
    );
}

#[test]
fn handlers_are_transformed() {
    let output = typeway_migrate::typeway_to_axum(FIXTURE).expect("conversion should succeed");

    // Path extractors should use Axum-style destructuring: Path(id): Path<u32>
    assert!(
        output.contains("Path(id)") || output.contains("Path (id)"),
        "should contain Axum-style Path destructuring, got:\n{output}"
    );

    // Should NOT contain typeway-style `path.0` destructuring.
    assert!(
        !output.contains("path.0") && !output.contains("path . 0"),
        "should not contain typeway-style path.0 destructuring, got:\n{output}"
    );

    // Should NOT contain `let state = state.0;`
    assert!(
        !output.contains("state.0") && !output.contains("state . 0"),
        "should not contain typeway-style state.0 destructuring, got:\n{output}"
    );
}

#[test]
fn endpoints_grouped_by_path() {
    let output = typeway_migrate::typeway_to_axum(FIXTURE).expect("conversion should succeed");

    // Count .route( occurrences — should be 2 (one for /users, one for /users/{id}).
    let route_count = output.matches(".route(").count();
    assert_eq!(
        route_count, 2,
        "should have 2 .route() calls (grouped by path), got {} in:\n{}",
        route_count, output
    );
}
