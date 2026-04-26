use typeway_migrate::model::BindMacro;
use typeway_migrate::parse::axum::parse_axum_file;

const FIXTURE: &str = include_str!("fixtures/validation_axum.rs");

#[test]
fn validation_patterns_detected_in_register() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let register = model
        .endpoints
        .iter()
        .find(|ep| ep.handler.name == "register")
        .expect("should find register endpoint");

    assert!(
        register.has_validation,
        "register handler should have validation patterns detected"
    );
}

#[test]
fn validator_name_is_generated() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let register = model
        .endpoints
        .iter()
        .find(|ep| ep.handler.name == "register")
        .expect("should find register endpoint");

    assert_eq!(
        register.validator_name.as_deref(),
        Some("CreateUserValidator"),
        "validator name should be CreateUserValidator"
    );
}

#[test]
fn bind_validated_macro_is_used() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let register = model
        .endpoints
        .iter()
        .find(|ep| ep.handler.name == "register")
        .expect("should find register endpoint");

    assert_eq!(
        register.bind_macro,
        BindMacro::BindValidated,
        "register should use bind_validated! macro (no auth present)"
    );
}

#[test]
fn output_contains_validator_struct() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("CreateUserValidator"),
        "output should contain CreateUserValidator struct, got:\n{output}"
    );
}

#[test]
fn output_contains_validate_impl() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("Validate"),
        "output should contain Validate<CreateUser> impl, got:\n{output}"
    );
    assert!(
        output.contains("CreateUser"),
        "output should reference CreateUser in Validate impl, got:\n{output}"
    );
}

#[test]
fn output_contains_validated_wrapper_in_api_type() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("Validated"),
        "output should contain Validated<CreateUserValidator, ...> in API type, got:\n{output}"
    );
}

#[test]
fn output_contains_bind_validated() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("bind_validated"),
        "output should contain bind_validated! macro call, got:\n{output}"
    );
}

#[test]
fn output_contains_with_openapi() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("with_openapi"),
        "output should contain .with_openapi() call, got:\n{output}"
    );
}

#[test]
fn output_parses_as_valid_rust() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    // Strip comment lines that are not Rust syntax.
    let code: String = output
        .lines()
        .filter(|line| !line.starts_with("// TODO:"))
        .filter(|line| !line.starts_with("// Requires:"))
        .filter(|line| !line.starts_with("// OpenAPI"))
        .collect::<Vec<_>>()
        .join("\n");

    let parsed: Result<syn::File, _> = syn::parse_str(&code);
    assert!(
        parsed.is_ok(),
        "output should be valid Rust syntax: {:?}\ncode:\n{}",
        parsed.err(),
        code
    );
}
