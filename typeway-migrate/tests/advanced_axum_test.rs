use typeway_migrate::parse::axum::parse_axum_file;

const FIXTURE: &str = include_str!("fixtures/advanced_axum.rs");

#[test]
fn detects_nest_prefix() {
    let model = parse_axum_file(FIXTURE).expect("should parse");
    assert_eq!(
        model.prefix.as_deref(),
        Some("/api/v1"),
        "should detect the .nest(\"/api/v1\", ...) prefix"
    );
}

#[test]
fn from_fn_middleware_produces_warning() {
    let model = parse_axum_file(FIXTURE).expect("should parse");
    let from_fn_warnings: Vec<_> = model
        .warnings
        .iter()
        .filter(|w| w.contains("from_fn"))
        .collect();
    assert!(
        !from_fn_warnings.is_empty(),
        "should produce a warning for axum::middleware::from_fn, warnings: {:?}",
        model.warnings
    );
}

#[test]
fn impl_into_response_produces_warning() {
    let model = parse_axum_file(FIXTURE).expect("should parse");
    let into_response_warnings: Vec<_> = model
        .warnings
        .iter()
        .filter(|w| w.contains("impl IntoResponse"))
        .collect();
    assert!(
        !into_response_warnings.is_empty(),
        "should produce a warning for handlers returning impl IntoResponse, warnings: {:?}",
        model.warnings
    );
}

#[test]
fn custom_extractor_produces_warning() {
    let model = parse_axum_file(FIXTURE).expect("should parse");
    // CustomAuth is now recognized as an auth extractor, so it produces
    // an auth detection warning instead of the generic "unknown extractor" one.
    let extractor_warnings: Vec<_> = model
        .warnings
        .iter()
        .filter(|w| w.contains("unknown extractor") || w.contains("Detected auth extractor"))
        .collect();
    assert!(
        !extractor_warnings.is_empty(),
        "should produce a warning for custom extractors (auth or unknown), warnings: {:?}",
        model.warnings
    );
}

#[test]
fn conversion_succeeds_with_warnings() {
    // Warnings should not block the conversion.
    let output =
        typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    // The output should contain TODO comments from warnings.
    assert!(
        output.contains("// TODO:"),
        "output should contain TODO comments from warnings, got:\n{}",
        output
    );

    // The output should still contain the key typeway constructs.
    assert!(
        output.contains("type API"),
        "should contain API type declaration, got:\n{}",
        output
    );
    assert!(
        output.contains("Server"),
        "should contain Server construction, got:\n{}",
        output
    );
}

#[test]
fn nest_prefix_appears_in_output() {
    let output =
        typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");
    assert!(
        output.contains("/api/v1"),
        "output should contain the nest prefix, got:\n{}",
        output
    );
}

#[test]
fn nested_router_function_noted_in_warnings() {
    let model = parse_axum_file(FIXTURE).expect("should parse");
    let nested_warnings: Vec<_> = model
        .warnings
        .iter()
        .filter(|w| w.contains("api_routes"))
        .collect();
    assert!(
        !nested_warnings.is_empty(),
        "should note nested router function call in warnings, warnings: {:?}",
        model.warnings
    );
}
