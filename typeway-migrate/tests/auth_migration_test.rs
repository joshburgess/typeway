use typeway_migrate::model::{BindMacro, ExtractorKind};
use typeway_migrate::parse::axum::parse_axum_file;

const FIXTURE: &str = include_str!("fixtures/auth_axum.rs");

#[test]
fn list_users_is_not_detected_as_auth() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let list = model
        .endpoints
        .iter()
        .find(|ep| ep.handler.name == "list_users")
        .expect("should find list_users");

    assert!(
        !list.requires_auth,
        "list_users should NOT be detected as auth-protected"
    );
    assert_eq!(list.bind_macro, BindMacro::Bind);
    assert!(list.auth_type.is_none());
}

#[test]
fn get_user_is_detected_as_auth() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let get = model
        .endpoints
        .iter()
        .find(|ep| ep.handler.name == "get_user")
        .expect("should find get_user");

    assert!(
        get.requires_auth,
        "get_user should be detected as auth-protected"
    );
    assert_eq!(get.bind_macro, BindMacro::BindAuth);
    assert_eq!(get.auth_type.as_deref(), Some("AuthUser"));
}

#[test]
fn create_user_is_detected_as_auth() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let create = model
        .endpoints
        .iter()
        .find(|ep| ep.handler.name == "create_user")
        .expect("should find create_user");

    assert!(
        create.requires_auth,
        "create_user should be detected as auth-protected"
    );
    assert_eq!(create.bind_macro, BindMacro::BindAuth);
    assert_eq!(create.auth_type.as_deref(), Some("AuthUser"));
}

#[test]
fn delete_user_is_detected_as_auth() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let delete = model
        .endpoints
        .iter()
        .find(|ep| ep.handler.name == "delete_user")
        .expect("should find delete_user");

    assert!(
        delete.requires_auth,
        "delete_user should be detected as auth-protected"
    );
    assert_eq!(delete.bind_macro, BindMacro::BindAuth);
    assert_eq!(delete.auth_type.as_deref(), Some("AuthUser"));
}

#[test]
fn query_extractor_is_detected() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let list = model
        .endpoints
        .iter()
        .find(|ep| ep.handler.name == "list_users")
        .expect("should find list_users");

    let has_query = list
        .handler
        .extractors
        .iter()
        .any(|e| e.kind == ExtractorKind::Query);
    assert!(has_query, "list_users should have a Query extractor");
}

#[test]
fn full_conversion_contains_protected_wrappers() {
    let output =
        typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("Protected"),
        "output should contain Protected<...> wrappers, got:\n{output}"
    );
    assert!(
        output.contains("AuthUser"),
        "output should reference AuthUser type, got:\n{output}"
    );
}

#[test]
fn full_conversion_contains_bind_auth() {
    let output =
        typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("bind_auth"),
        "output should contain bind_auth! for auth endpoints, got:\n{output}"
    );
}

#[test]
fn full_conversion_contains_plain_bind_for_public() {
    let output =
        typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    // The list_users endpoint should use bind!, not bind_auth!.
    // We verify bind!(list_users) appears (for the public endpoint).
    assert!(
        output.contains("bind!(list_users)"),
        "output should contain bind!(list_users) for public endpoints, got:\n{output}"
    );
}

#[test]
fn query_is_passed_through_in_output() {
    let output =
        typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("Query"),
        "output should contain Query<Pagination> extractor, got:\n{output}"
    );
    assert!(
        output.contains("Pagination"),
        "output should reference Pagination type, got:\n{output}"
    );
}

#[test]
fn output_parses_as_valid_rust() {
    let output =
        typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    // Strip TODO comment lines — they are not Rust syntax.
    let code: String = output
        .lines()
        .filter(|line| !line.starts_with("// TODO:"))
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

#[test]
fn auth_warnings_are_produced() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let auth_warnings: Vec<_> = model
        .warnings
        .iter()
        .filter(|w| w.contains("Detected auth extractor"))
        .collect();

    assert!(
        !auth_warnings.is_empty(),
        "should produce warnings for detected auth extractors, warnings: {:?}",
        model.warnings
    );
}
