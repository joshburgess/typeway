//! Roundtrip tests: Axum -> Typeway -> Axum and Typeway -> Axum -> Typeway.
//!
//! These tests verify that converting back and forth preserves the essential
//! structural properties of the API: handler names, route paths, HTTP methods,
//! and endpoint count. Exact string equality is not expected.

use std::collections::BTreeSet;

use typeway_migrate::model::HttpMethod;
use typeway_migrate::parse;

const AXUM_FIXTURE: &str = include_str!("fixtures/simple_axum.rs");
const TYPEWAY_FIXTURE: &str = include_str!("fixtures/simple_typeway.rs");

/// Structural summary of an API extracted from source code.
#[derive(Debug)]
struct ApiSummary {
    handler_names: BTreeSet<String>,
    route_paths: BTreeSet<String>,
    methods: BTreeSet<String>,
    endpoint_count: usize,
}

/// Extract a structural summary from an Axum source string.
fn summarize_axum(source: &str) -> ApiSummary {
    let model = parse::axum::parse_axum_file(source).expect("should parse Axum source");
    ApiSummary {
        handler_names: model
            .endpoints
            .iter()
            .map(|ep| ep.handler.name.to_string())
            .collect(),
        route_paths: model
            .endpoints
            .iter()
            .map(|ep| normalize_path(&ep.path.raw_pattern))
            .collect(),
        methods: model
            .endpoints
            .iter()
            .map(|ep| method_str(ep.method))
            .collect(),
        endpoint_count: model.endpoints.len(),
    }
}

/// Extract a structural summary from a Typeway source string.
fn summarize_typeway(source: &str) -> ApiSummary {
    let model =
        parse::typeway::parse_typeway_file(source).expect("should parse Typeway source");
    ApiSummary {
        handler_names: model
            .endpoints
            .iter()
            .map(|ep| ep.handler.name.to_string())
            .collect(),
        route_paths: model
            .endpoints
            .iter()
            .map(|ep| normalize_path(&ep.path.raw_pattern))
            .collect(),
        methods: model
            .endpoints
            .iter()
            .map(|ep| method_str(ep.method))
            .collect(),
        endpoint_count: model.endpoints.len(),
    }
}

/// Normalize path patterns so that `/users/{id}` and `/users/{u32}` compare
/// equal. Replaces capture names with a positional placeholder.
fn normalize_path(raw: &str) -> String {
    let mut idx = 0;
    let parts: Vec<String> = raw
        .split('/')
        .map(|seg| {
            if seg.starts_with('{') && seg.ends_with('}') {
                idx += 1;
                format!("{{_{}}}", idx)
            } else {
                seg.to_string()
            }
        })
        .collect();
    parts.join("/")
}

fn method_str(m: HttpMethod) -> String {
    format!("{:?}", m)
}

/// Assert the output parses as valid Rust syntax.
fn assert_valid_rust(source: &str, label: &str) {
    let result: Result<syn::File, _> = syn::parse_str(source);
    assert!(
        result.is_ok(),
        "{} should produce valid Rust syntax: {:?}\n\nSource:\n{}",
        label,
        result.err(),
        source
    );
}

/// Assert two summaries have matching structural properties.
fn assert_summaries_match(original: &ApiSummary, roundtripped: &ApiSummary, label: &str) {
    assert_eq!(
        original.endpoint_count, roundtripped.endpoint_count,
        "{}: endpoint count mismatch (original {} vs roundtripped {})",
        label, original.endpoint_count, roundtripped.endpoint_count
    );

    assert_eq!(
        original.handler_names, roundtripped.handler_names,
        "{}: handler names differ.\n  original:    {:?}\n  roundtripped: {:?}",
        label, original.handler_names, roundtripped.handler_names
    );

    assert_eq!(
        original.route_paths, roundtripped.route_paths,
        "{}: route paths differ.\n  original:    {:?}\n  roundtripped: {:?}",
        label, original.route_paths, roundtripped.route_paths
    );

    assert_eq!(
        original.methods, roundtripped.methods,
        "{}: HTTP methods differ.\n  original:    {:?}\n  roundtripped: {:?}",
        label, original.methods, roundtripped.methods
    );
}

// ---------------------------------------------------------------------------
// Axum -> Typeway -> Axum roundtrip
// ---------------------------------------------------------------------------

#[test]
fn axum_to_typeway_to_axum_roundtrip() {
    // Step 1: Parse the original Axum fixture to get the ground truth.
    let original_summary = summarize_axum(AXUM_FIXTURE);

    // Step 2: Convert Axum -> Typeway.
    let typeway_source =
        typeway_migrate::axum_to_typeway(AXUM_FIXTURE).expect("Axum -> Typeway should succeed");
    assert_valid_rust(&typeway_source, "Axum -> Typeway output");

    // Step 3: Convert the Typeway output back to Axum.
    let axum_roundtripped = typeway_migrate::typeway_to_axum(&typeway_source)
        .expect("Typeway -> Axum roundtrip should succeed");
    assert_valid_rust(&axum_roundtripped, "Axum -> Typeway -> Axum output");

    // Step 4: Parse the roundtripped Axum source and compare structurally.
    let roundtripped_summary = summarize_axum(&axum_roundtripped);
    assert_summaries_match(
        &original_summary,
        &roundtripped_summary,
        "Axum -> Typeway -> Axum",
    );
}

// ---------------------------------------------------------------------------
// Typeway -> Axum -> Typeway roundtrip
// ---------------------------------------------------------------------------

#[test]
fn typeway_to_axum_to_typeway_roundtrip() {
    // Step 1: Parse the original Typeway fixture to get the ground truth.
    let original_summary = summarize_typeway(TYPEWAY_FIXTURE);

    // Step 2: Convert Typeway -> Axum.
    let axum_source = typeway_migrate::typeway_to_axum(TYPEWAY_FIXTURE)
        .expect("Typeway -> Axum should succeed");
    assert_valid_rust(&axum_source, "Typeway -> Axum output");

    // Step 3: Convert the Axum output back to Typeway.
    let typeway_roundtripped = typeway_migrate::axum_to_typeway(&axum_source)
        .expect("Axum -> Typeway roundtrip should succeed");
    assert_valid_rust(&typeway_roundtripped, "Typeway -> Axum -> Typeway output");

    // Step 4: Parse the roundtripped Typeway source and compare structurally.
    let roundtripped_summary = summarize_typeway(&typeway_roundtripped);
    assert_summaries_match(
        &original_summary,
        &roundtripped_summary,
        "Typeway -> Axum -> Typeway",
    );
}

// ---------------------------------------------------------------------------
// Intermediate conversion validity checks
// ---------------------------------------------------------------------------

#[test]
fn axum_to_typeway_intermediate_has_expected_constructs() {
    let typeway_source =
        typeway_migrate::axum_to_typeway(AXUM_FIXTURE).expect("conversion should succeed");

    // The intermediate Typeway source should have these structural markers.
    assert!(
        typeway_source.contains("typeway_path"),
        "intermediate should contain typeway_path! declarations"
    );
    assert!(
        typeway_source.contains("type API"),
        "intermediate should contain API type alias"
    );
    assert!(
        typeway_source.contains("Server"),
        "intermediate should contain Server construction"
    );
}

#[test]
fn typeway_to_axum_intermediate_has_expected_constructs() {
    let axum_source = typeway_migrate::typeway_to_axum(TYPEWAY_FIXTURE)
        .expect("conversion should succeed");

    // The intermediate Axum source should have these structural markers.
    let has_router = axum_source.contains("Router") || axum_source.contains("router");
    assert!(
        has_router,
        "intermediate should contain Router, got:\n{axum_source}"
    );

    let has_route_call = axum_source.contains(".route(") || axum_source.contains(". route(");
    assert!(
        has_route_call,
        "intermediate should contain .route( calls, got:\n{axum_source}"
    );
}
