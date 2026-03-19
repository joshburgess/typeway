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
const AUTH_AXUM_FIXTURE: &str = include_str!("fixtures/auth_axum.rs");
const EFFECTS_AXUM_FIXTURE: &str = include_str!("fixtures/effects_axum.rs");
const FULL_FEATURED_AXUM_FIXTURE: &str = include_str!("fixtures/full_featured_axum.rs");

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

// ---------------------------------------------------------------------------
// Auth roundtrip: Axum -> Typeway -> Axum
// ---------------------------------------------------------------------------

#[test]
fn auth_axum_to_typeway_to_axum_roundtrip() {
    // Step 1: Parse the original auth Axum fixture.
    let original_summary = summarize_axum(AUTH_AXUM_FIXTURE);

    // Step 2: Convert Axum -> Typeway.
    let typeway_source = typeway_migrate::axum_to_typeway(AUTH_AXUM_FIXTURE)
        .expect("Auth Axum -> Typeway should succeed");

    // Verify auth constructs in the intermediate Typeway output.
    assert!(
        typeway_source.contains("Protected"),
        "intermediate Typeway should contain Protected<> wrappers, got:\n{typeway_source}"
    );
    assert!(
        typeway_source.contains("bind_auth"),
        "intermediate Typeway should contain bind_auth! macro calls, got:\n{typeway_source}"
    );

    // Step 3: Convert the Typeway output back to Axum.
    let axum_roundtripped = typeway_migrate::typeway_to_axum(&typeway_source)
        .expect("Auth Typeway -> Axum roundtrip should succeed");
    assert_valid_rust(&axum_roundtripped, "Auth Axum -> Typeway -> Axum output");

    // Step 4: Parse the roundtripped Axum source and compare structurally.
    let roundtripped_summary = summarize_axum(&axum_roundtripped);
    assert_summaries_match(
        &original_summary,
        &roundtripped_summary,
        "Auth Axum -> Typeway -> Axum",
    );

    // Step 5: Verify auth endpoints still have auth extractor as first argument.
    let roundtripped_model = parse::axum::parse_axum_file(&axum_roundtripped)
        .expect("should parse roundtripped auth Axum");
    let auth_endpoints: Vec<_> = roundtripped_model
        .endpoints
        .iter()
        .filter(|ep| ep.requires_auth)
        .collect();
    // The original has 3 protected endpoints: get_user, create_user, delete_user.
    assert!(
        auth_endpoints.len() >= 3,
        "roundtripped source should preserve at least 3 auth endpoints, found {}",
        auth_endpoints.len()
    );
}

#[test]
fn auth_intermediate_typeway_has_protected_wrappers() {
    let typeway_source = typeway_migrate::axum_to_typeway(AUTH_AXUM_FIXTURE)
        .expect("conversion should succeed");

    // Count Protected<> occurrences (there should be at least 3).
    let protected_count = typeway_source.matches("Protected").count();
    assert!(
        protected_count >= 3,
        "expected at least 3 Protected<> wrappers, found {}",
        protected_count
    );

    assert!(
        typeway_source.contains("AuthUser"),
        "intermediate should reference AuthUser type"
    );
}

// ---------------------------------------------------------------------------
// Effects roundtrip: Axum -> Typeway -> Axum
// ---------------------------------------------------------------------------

#[test]
fn effects_axum_to_typeway_to_axum_roundtrip() {
    // Step 1: Parse the original effects Axum fixture.
    let original_summary = summarize_axum(EFFECTS_AXUM_FIXTURE);

    // Step 2: Convert Axum -> Typeway.
    let typeway_source = typeway_migrate::axum_to_typeway(EFFECTS_AXUM_FIXTURE)
        .expect("Effects Axum -> Typeway should succeed");

    // Verify effects constructs in the intermediate Typeway output.
    assert!(
        typeway_source.contains("EffectfulServer"),
        "intermediate Typeway should contain EffectfulServer, got:\n{typeway_source}"
    );
    assert!(
        typeway_source.contains("Requires"),
        "intermediate Typeway should contain Requires<> wrappers, got:\n{typeway_source}"
    );
    assert!(
        typeway_source.contains("CorsRequired"),
        "intermediate Typeway should contain CorsRequired effect, got:\n{typeway_source}"
    );

    // Step 3: Convert the Typeway output back to Axum.
    let axum_roundtripped = typeway_migrate::typeway_to_axum(&typeway_source)
        .expect("Effects Typeway -> Axum roundtrip should succeed");
    assert_valid_rust(
        &axum_roundtripped,
        "Effects Axum -> Typeway -> Axum output",
    );

    // Step 4: Parse the roundtripped Axum source and compare structurally.
    let roundtripped_summary = summarize_axum(&axum_roundtripped);
    assert_summaries_match(
        &original_summary,
        &roundtripped_summary,
        "Effects Axum -> Typeway -> Axum",
    );

    // Step 5: Verify layer calls survive the roundtrip.
    let roundtripped_model = parse::axum::parse_axum_file(&axum_roundtripped)
        .expect("should parse roundtripped effects Axum");
    assert!(
        !roundtripped_model.layers.is_empty(),
        "roundtripped source should preserve layer calls"
    );
}

#[test]
fn effects_intermediate_typeway_has_effect_markers() {
    let typeway_source = typeway_migrate::axum_to_typeway(EFFECTS_AXUM_FIXTURE)
        .expect("conversion should succeed");

    // Verify both CORS and Tracing effects are present.
    assert!(
        typeway_source.contains("CorsRequired"),
        "intermediate should contain CorsRequired"
    );
    assert!(
        typeway_source.contains("TracingRequired"),
        "intermediate should contain TracingRequired"
    );

    // Verify .provide calls are present.
    assert!(
        typeway_source.contains("provide"),
        "intermediate should contain .provide() calls"
    );
}

// ---------------------------------------------------------------------------
// Full-featured roundtrip: Axum -> Typeway -> Axum
// ---------------------------------------------------------------------------

#[test]
fn full_featured_axum_to_typeway_to_axum_roundtrip() {
    // Step 1: Parse the original full-featured Axum fixture.
    let original_summary = summarize_axum(FULL_FEATURED_AXUM_FIXTURE);

    // Step 2: Convert Axum -> Typeway.
    let typeway_source = typeway_migrate::axum_to_typeway(FULL_FEATURED_AXUM_FIXTURE)
        .expect("Full-featured Axum -> Typeway should succeed");

    // Verify all feature types are represented in the intermediate.
    assert!(
        typeway_source.contains("Protected"),
        "intermediate should contain Protected<> for auth endpoints"
    );
    assert!(
        typeway_source.contains("bind_auth"),
        "intermediate should contain bind_auth! macro calls"
    );
    assert!(
        typeway_source.contains("EffectfulServer") || typeway_source.contains("Server"),
        "intermediate should contain Server or EffectfulServer"
    );
    assert!(
        typeway_source.contains("typeway_path"),
        "intermediate should contain typeway_path! declarations"
    );
    assert!(
        typeway_source.contains("type API"),
        "intermediate should contain API type alias"
    );

    // Step 3: Convert the Typeway output back to Axum.
    let axum_roundtripped = typeway_migrate::typeway_to_axum(&typeway_source)
        .expect("Full-featured Typeway -> Axum roundtrip should succeed");
    assert_valid_rust(
        &axum_roundtripped,
        "Full-featured Axum -> Typeway -> Axum output",
    );

    // Step 4: Parse the roundtripped Axum source and compare structurally.
    let roundtripped_summary = summarize_axum(&axum_roundtripped);
    assert_summaries_match(
        &original_summary,
        &roundtripped_summary,
        "Full-featured Axum -> Typeway -> Axum",
    );
}

#[test]
fn full_featured_intermediate_preserves_all_features() {
    let typeway_source = typeway_migrate::axum_to_typeway(FULL_FEATURED_AXUM_FIXTURE)
        .expect("conversion should succeed");

    // Auth detection.
    let protected_count = typeway_source.matches("Protected").count();
    assert!(
        protected_count >= 3,
        "expected at least 3 Protected<> wrappers for auth endpoints, found {}",
        protected_count
    );

    // Effects detection (from CorsLayer and TraceLayer).
    assert!(
        typeway_source.contains("CorsRequired"),
        "intermediate should contain CorsRequired effect"
    );
    assert!(
        typeway_source.contains("TracingRequired"),
        "intermediate should contain TracingRequired effect"
    );

    // AuthUser should be referenced.
    assert!(
        typeway_source.contains("AuthUser"),
        "intermediate should reference AuthUser type"
    );
}

#[test]
fn full_featured_model_has_correct_auth_counts() {
    let model = parse::axum::parse_axum_file(FULL_FEATURED_AXUM_FIXTURE)
        .expect("should parse full-featured Axum");

    let auth_count = model.endpoints.iter().filter(|ep| ep.requires_auth).count();
    let public_count = model.endpoints.iter().filter(|ep| !ep.requires_auth).count();

    assert_eq!(auth_count, 3, "expected 3 auth endpoints (get_user, create_user, delete_user)");
    assert_eq!(public_count, 1, "expected 1 public endpoint (list_users)");

    // Verify auth type is detected as AuthUser.
    for ep in model.endpoints.iter().filter(|ep| ep.requires_auth) {
        assert_eq!(
            ep.auth_type.as_deref(),
            Some("AuthUser"),
            "endpoint {} should have auth_type = AuthUser",
            ep.handler.name
        );
    }
}

#[test]
fn full_featured_model_has_correct_effects() {
    let model = parse::axum::parse_axum_file(FULL_FEATURED_AXUM_FIXTURE)
        .expect("should parse full-featured Axum");

    let effect_names: BTreeSet<String> = model
        .detected_effects
        .iter()
        .map(|e| e.effect_name.clone())
        .collect();

    assert!(
        effect_names.contains("CorsRequired"),
        "should detect CorsRequired effect"
    );
    assert!(
        effect_names.contains("TracingRequired"),
        "should detect TracingRequired effect"
    );
}

#[test]
fn full_featured_model_detects_query_extractors() {
    let model = parse::axum::parse_axum_file(FULL_FEATURED_AXUM_FIXTURE)
        .expect("should parse full-featured Axum");

    let query_endpoints: Vec<_> = model
        .endpoints
        .iter()
        .filter(|ep| {
            ep.handler
                .extractors
                .iter()
                .any(|e| e.kind == typeway_migrate::model::ExtractorKind::Query)
        })
        .collect();

    assert_eq!(
        query_endpoints.len(),
        1,
        "expected 1 endpoint with Query extractor (list_users)"
    );
    assert_eq!(
        query_endpoints[0].handler.name.to_string(),
        "list_users",
        "the Query endpoint should be list_users"
    );
}

#[test]
fn full_featured_model_detects_state() {
    let model = parse::axum::parse_axum_file(FULL_FEATURED_AXUM_FIXTURE)
        .expect("should parse full-featured Axum");

    assert!(
        model.state_type.is_some(),
        "should detect state type from .with_state() call"
    );
}
