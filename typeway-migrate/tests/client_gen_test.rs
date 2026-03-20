use typeway_migrate::parse::axum::parse_axum_file;
use typeway_migrate::transform::axum_to_typeway::emit_client_api_string;

const SIMPLE_FIXTURE: &str = include_str!("fixtures/simple_axum.rs");
const AUTH_FIXTURE: &str = include_str!("fixtures/auth_axum.rs");

#[test]
fn client_api_string_contains_macro_invocation() {
    let model = parse_axum_file(SIMPLE_FIXTURE).expect("should parse");
    let output = emit_client_api_string(&model);
    assert!(
        output.contains("client_api!"),
        "output should contain client_api! macro invocation, got:\n{}",
        output
    );
}

#[test]
fn client_api_string_contains_handler_method_names() {
    let model = parse_axum_file(SIMPLE_FIXTURE).expect("should parse");
    let output = emit_client_api_string(&model);

    assert!(
        output.contains("list_users"),
        "output should contain list_users method name"
    );
    assert!(
        output.contains("get_user"),
        "output should contain get_user method name"
    );
    assert!(
        output.contains("create_user"),
        "output should contain create_user method name"
    );
    assert!(
        output.contains("delete_user"),
        "output should contain delete_user method name"
    );
}

#[test]
fn client_api_string_contains_endpoint_types() {
    let model = parse_axum_file(SIMPLE_FIXTURE).expect("should parse");
    let output = emit_client_api_string(&model);

    assert!(
        output.contains("GetEndpoint"),
        "output should contain GetEndpoint type"
    );
    assert!(
        output.contains("PostEndpoint"),
        "output should contain PostEndpoint type"
    );
    assert!(
        output.contains("DeleteEndpoint"),
        "output should contain DeleteEndpoint type"
    );
}

#[test]
fn protected_endpoints_use_inner_endpoint_type() {
    let model = parse_axum_file(AUTH_FIXTURE).expect("should parse");
    let output = emit_client_api_string(&model);

    // The client_api! output should NOT contain Protected wrappers.
    assert!(
        !output.contains("Protected"),
        "client_api! output should not contain Protected wrapper, got:\n{}",
        output
    );

    // But it should still contain the endpoint types for auth-protected handlers.
    assert!(
        output.contains("get_user"),
        "output should contain get_user method name"
    );
    assert!(
        output.contains("delete_user"),
        "output should contain delete_user method name"
    );
    assert!(
        output.contains("GetEndpoint"),
        "output should contain GetEndpoint for protected GET endpoints"
    );
    assert!(
        output.contains("DeleteEndpoint"),
        "output should contain DeleteEndpoint for protected DELETE endpoints"
    );
}

#[test]
fn empty_model_produces_empty_string() {
    let source = r#"
        fn not_an_api() {}
    "#;
    // If parsing produces no endpoints, client API string should be empty.
    match parse_axum_file(source) {
        Ok(model) => {
            let output = emit_client_api_string(&model);
            assert!(
                output.is_empty(),
                "empty model should produce empty client API string"
            );
        }
        Err(_) => {
            // Parsing failure is acceptable for non-API code.
        }
    }
}

#[test]
fn client_api_string_is_commented_out() {
    let model = parse_axum_file(SIMPLE_FIXTURE).expect("should parse");
    let output = emit_client_api_string(&model);

    // Every non-empty line should start with "//"
    for line in output.lines() {
        assert!(
            line.starts_with("//"),
            "all lines in client_api output should be comments, but found: {:?}",
            line
        );
    }
}
