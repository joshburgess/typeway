use typeway_migrate::parse::axum::parse_axum_file;

const FIXTURE: &str = include_str!("fixtures/merged_axum.rs");

#[test]
fn finds_all_endpoints_from_merged_functions() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    assert_eq!(
        model.endpoints.len(),
        5,
        "should find 5 endpoints (3 from user_routes + 2 from article_routes), got: {:?}",
        model
            .endpoints
            .iter()
            .map(|ep| format!("{:?} {}", ep.method, ep.path.raw_pattern))
            .collect::<Vec<_>>()
    );
}

#[test]
fn extracts_all_handler_names() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let names: Vec<_> = model
        .endpoints
        .iter()
        .map(|ep| ep.handler.name.to_string())
        .collect();

    assert!(names.contains(&"list_users".to_string()), "missing list_users");
    assert!(names.contains(&"get_user".to_string()), "missing get_user");
    assert!(names.contains(&"create_user".to_string()), "missing create_user");
    assert!(names.contains(&"list_articles".to_string()), "missing list_articles");
    assert!(names.contains(&"delete_article".to_string()), "missing delete_article");
}

#[test]
fn extracts_correct_path_patterns() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    let paths: Vec<_> = model
        .endpoints
        .iter()
        .map(|ep| ep.path.raw_pattern.clone())
        .collect();

    assert!(paths.contains(&"/users".to_string()), "missing /users");
    assert!(paths.contains(&"/users/{id}".to_string()), "missing /users/{{id}}");
    assert!(paths.contains(&"/articles".to_string()), "missing /articles");
    assert!(
        paths.contains(&"/articles/{id}".to_string()),
        "missing /articles/{{id}}"
    );
}

#[test]
fn full_conversion_produces_valid_rust_with_all_endpoints() {
    let output =
        typeway_migrate::axum_to_typeway(FIXTURE).expect("conversion should succeed");

    // The output should parse as valid Rust syntax.
    let parsed: Result<syn::File, _> = syn::parse_str(&output);
    assert!(
        parsed.is_ok(),
        "output should be valid Rust syntax: {:?}\n\nOutput:\n{}",
        parsed.err(),
        output
    );

    // Should contain the API type with all endpoints.
    assert!(
        output.contains("type API"),
        "should contain API type declaration, got:\n{output}"
    );

    // Should reference both user and article path types.
    assert!(
        output.contains("UsersPath") || output.contains("users"),
        "should contain user paths, got:\n{output}"
    );
    assert!(
        output.contains("ArticlesPath") || output.contains("articles"),
        "should contain article paths, got:\n{output}"
    );
}

#[test]
fn check_reports_all_endpoints() {
    let model = parse_axum_file(FIXTURE).expect("should parse");

    // Simulates what the check command does: count endpoints.
    assert_eq!(model.endpoints.len(), 5);

    let methods: Vec<_> = model
        .endpoints
        .iter()
        .map(|ep| format!("{:?} {}", ep.method, ep.path.raw_pattern))
        .collect();

    assert!(methods.contains(&"Get /users".to_string()));
    assert!(methods.contains(&"Post /users".to_string()));
    assert!(methods.contains(&"Get /users/{id}".to_string()));
    assert!(methods.contains(&"Get /articles".to_string()));
    assert!(methods.contains(&"Delete /articles/{id}".to_string()));
}

#[test]
fn nest_with_function_applies_prefix() {
    let source = r#"
use axum::{routing::get, Router};

async fn health() -> &'static str { "ok" }

fn api_routes() -> Router {
    Router::new()
        .route("/health", get(health))
}

fn app() -> Router {
    Router::new()
        .nest("/api/v1", api_routes())
}
"#;

    let model = parse_axum_file(source).expect("should parse");

    assert_eq!(model.endpoints.len(), 1);
    assert_eq!(model.endpoints[0].path.raw_pattern, "/api/v1/health");
    assert_eq!(model.endpoints[0].handler.name.to_string(), "health");
}

#[test]
fn unresolvable_merge_emits_warning() {
    let source = r#"
use axum::{routing::get, Router};

fn app() -> Router {
    Router::new()
        .merge(external_routes())
}
"#;

    let model = parse_axum_file(source).expect("should parse");

    assert!(
        model
            .warnings
            .iter()
            .any(|w| w.contains("external_routes") && w.contains("not found")),
        "should warn about unresolvable merge, warnings: {:?}",
        model.warnings,
    );
}
