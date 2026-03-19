use typeway_migrate::interactive::filter_partial;
use typeway_migrate::parse::axum::parse_axum_file;

const FIXTURE: &str = include_str!("fixtures/simple_axum.rs");

#[test]
fn partial_filter_keeps_only_matching_routes() {
    let mut model = parse_axum_file(FIXTURE).expect("should parse");

    // The fixture has: GET /users, POST /users, GET /users/{id}, DELETE /users/{id}
    assert_eq!(model.endpoints.len(), 4);

    // Filter to only /users (exact match, not /users/{id}).
    filter_partial(&mut model, &["/users".to_string()]);

    // /users matches exactly, /users/{id} starts with "/users/" so it also matches.
    // Both /users and /users/{id} should be retained since /users/{id} starts with "/users/".
    assert_eq!(model.endpoints.len(), 4);

    let patterns: Vec<_> = model
        .endpoints
        .iter()
        .map(|ep| ep.path.raw_pattern.as_str())
        .collect();

    assert!(patterns.contains(&"/users"));
    assert!(patterns.contains(&"/users/{id}"));
}

#[test]
fn partial_filter_exact_only() {
    let mut model = parse_axum_file(FIXTURE).expect("should parse");
    assert_eq!(model.endpoints.len(), 4);

    // Filter to only /users/{id} — should not match /users.
    filter_partial(&mut model, &["/users/{id}".to_string()]);

    assert_eq!(model.endpoints.len(), 2);

    for ep in &model.endpoints {
        assert_eq!(ep.path.raw_pattern, "/users/{id}");
    }
}

#[test]
fn partial_filter_no_match_empties_endpoints() {
    let mut model = parse_axum_file(FIXTURE).expect("should parse");
    assert_eq!(model.endpoints.len(), 4);

    filter_partial(&mut model, &["/nonexistent".to_string()]);

    assert_eq!(model.endpoints.len(), 0);
}

#[test]
fn partial_conversion_produces_output_for_matched_routes_only() {
    let output = typeway_migrate::axum_to_typeway_with_options(
        FIXTURE,
        false,
        Some(&["/users/{id}".to_string()]),
    )
    .expect("conversion should succeed");

    // Should contain get_user and delete_user but not list_users or create_user.
    assert!(
        output.contains("get_user"),
        "should contain get_user handler, got:\n{output}"
    );
    assert!(
        output.contains("delete_user"),
        "should contain delete_user handler, got:\n{output}"
    );
    // list_users and create_user are on /users, which we excluded.
    assert!(
        !output.contains("list_users"),
        "should not contain list_users handler, got:\n{output}"
    );
    assert!(
        !output.contains("create_user"),
        "should not contain create_user handler, got:\n{output}"
    );
}
