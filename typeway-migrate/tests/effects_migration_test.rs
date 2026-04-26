use typeway_migrate::parse::axum::parse_axum_file;

const FIXTURE: &str = include_str!("fixtures/effects_axum.rs");

#[test]
fn detects_cors_effect() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let cors_effects: Vec<_> = model
        .detected_effects
        .iter()
        .filter(|e| e.effect_name == "CorsRequired")
        .collect();

    assert_eq!(
        cors_effects.len(),
        1,
        "should detect exactly one CorsRequired effect, got: {:?}",
        model.detected_effects,
    );
}

#[test]
fn detects_tracing_effect() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let trace_effects: Vec<_> = model
        .detected_effects
        .iter()
        .filter(|e| e.effect_name == "TracingRequired")
        .collect();

    assert_eq!(
        trace_effects.len(),
        1,
        "should detect exactly one TracingRequired effect, got: {:?}",
        model.detected_effects,
    );
}

#[test]
fn output_contains_effectful_server() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("EffectfulServer"),
        "output should contain EffectfulServer, got:\n{output}"
    );
    // Verify the output does NOT use plain Server (without Effectful prefix).
    // We check that every occurrence of "Server" in the server construction
    // is preceded by "Effectful".
    let server_construction_uses_effectful = output.contains("EffectfulServer");
    assert!(
        server_construction_uses_effectful,
        "output should use EffectfulServer, got:\n{output}"
    );
    // Count occurrences: "Server::" should only appear as part of "EffectfulServer::"
    let plain_server_count = output.matches("Server::").count();
    let effectful_server_count = output.matches("EffectfulServer::").count();
    assert_eq!(
        plain_server_count, effectful_server_count,
        "all Server:: occurrences should be EffectfulServer::, got:\n{output}"
    );
}

#[test]
fn output_contains_provide_cors() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("provide"),
        "output should contain .provide::<CorsRequired>(), got:\n{output}"
    );
    assert!(
        output.contains("CorsRequired"),
        "output should contain CorsRequired, got:\n{output}"
    );
}

#[test]
fn output_contains_requires_wrapper_in_api_type() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("Requires"),
        "output should contain Requires<CorsRequired, ...> in the API type, got:\n{output}"
    );
}

#[test]
fn output_contains_ready_call() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("ready"),
        "output should contain .ready() call, got:\n{output}"
    );
}

#[test]
fn output_parses_as_valid_rust() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    let parsed: Result<syn::File, _> = syn::parse_str(&output);
    assert!(
        parsed.is_ok(),
        "output should be valid Rust syntax: {:?}\n\nFull output:\n{output}",
        parsed.err()
    );
}

#[test]
fn output_contains_effect_imports() {
    let output = typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    assert!(
        output.contains("typeway_core"),
        "output should contain typeway_core::effects import, got:\n{output}"
    );
    assert!(
        output.contains("EffectfulServer"),
        "output should contain EffectfulServer import, got:\n{output}"
    );
    assert!(
        output.contains("Requires"),
        "output should contain Requires import, got:\n{output}"
    );
}
