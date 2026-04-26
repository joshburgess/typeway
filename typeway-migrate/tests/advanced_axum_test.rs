use typeway_migrate::parse::axum::parse_axum_file;

const FIXTURE: &str = include_str!("fixtures/advanced_axum.rs");

#[test]
fn resolves_nested_routes_with_prefix() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    // When .nest("/api/v1", api_routes()) is used and api_routes is in the
    // same file, the parser now resolves the function and applies the prefix
    // directly to each route's path pattern.
    let paths: Vec<_> = model
        .endpoints
        .iter()
        .map(|ep| ep.path.raw_pattern.clone())
        .collect();

    assert!(
        paths.iter().all(|p| p.starts_with("/api/v1")),
        "all paths should be prefixed with /api/v1, got: {:?}",
        paths,
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
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

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
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");
    // The nest prefix is now baked into path type names and path macro segments.
    assert!(
        output.contains("api") && output.contains("v1"),
        "output should contain the nest prefix segments, got:\n{}",
        output
    );
}

#[test]
fn nested_router_function_resolved_without_warning() {
    let model = parse_axum_file(FIXTURE).expect("should parse");
    // api_routes() is in the same file and is resolved via .nest(), so
    // there should be NO warning about it being unresolvable.
    let unresolved_warnings: Vec<_> = model
        .warnings
        .iter()
        .filter(|w| w.contains("api_routes") && w.contains("not found"))
        .collect();
    assert!(
        unresolved_warnings.is_empty(),
        "api_routes should be resolved without warnings, got: {:?}",
        unresolved_warnings,
    );

    // The routes from api_routes() should be present with the /api/v1 prefix.
    assert!(
        model.endpoints.len() == 3,
        "should have 3 endpoints (resolved from api_routes), got {}",
        model.endpoints.len(),
    );
}
