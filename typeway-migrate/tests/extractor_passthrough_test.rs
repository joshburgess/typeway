//! Tests for new extractor kinds: Cookie, CookieJar, Multipart, Form, WebSocketUpgrade.

use typeway_migrate::model::ExtractorKind;
use typeway_migrate::parse::axum::parse_axum_file;

/// Helper: classify a type name by building a minimal syn::Path and calling from_type_path.
fn classify(type_name: &str) -> ExtractorKind {
    let path: syn::Path = syn::parse_str(type_name).expect("valid path");
    ExtractorKind::from_type_path(&path)
}

#[test]
fn cookie_is_classified_correctly() {
    assert_eq!(classify("Cookie"), ExtractorKind::Cookie);
}

#[test]
fn cookie_jar_is_classified_correctly() {
    assert_eq!(classify("CookieJar"), ExtractorKind::CookieJar);
}

#[test]
fn multipart_is_classified_correctly() {
    assert_eq!(classify("Multipart"), ExtractorKind::Multipart);
}

#[test]
fn form_is_classified_correctly() {
    assert_eq!(classify("Form"), ExtractorKind::Form);
}

#[test]
fn websocket_upgrade_is_classified_correctly() {
    assert_eq!(classify("WebSocketUpgrade"), ExtractorKind::WebSocketUpgrade);
}

#[test]
fn cookie_extractor_passthrough_in_conversion() {
    let source = r#"
        use axum::{Router, routing::get};
        use axum_extra::extract::CookieJar;

        async fn get_cookies(jar: CookieJar) -> String {
            "cookies".to_string()
        }

        fn app() -> Router {
            Router::new()
                .route("/cookies", get(get_cookies))
        }
    "#;

    let model = parse_axum_file(source).expect("should parse");
    assert_eq!(model.endpoints.len(), 1);

    let ep = &model.endpoints[0];
    assert_eq!(ep.handler.extractors.len(), 1);
    assert_eq!(ep.handler.extractors[0].kind, ExtractorKind::CookieJar);

    // Verify it converts without errors in both directions.
    let typeway_output = typeway_migrate::axum_to_typeway(source);
    assert!(typeway_output.is_ok(), "axum_to_typeway should succeed");
    let output = typeway_output.unwrap();
    assert!(output.contains("CookieJar"), "output should contain CookieJar");
}

#[test]
fn form_extractor_passthrough_in_conversion() {
    let source = r#"
        use axum::{Router, routing::post, Form};

        #[derive(serde::Deserialize)]
        struct LoginForm {
            username: String,
            password: String,
        }

        async fn login(form: Form<LoginForm>) -> String {
            "logged in".to_string()
        }

        fn app() -> Router {
            Router::new()
                .route("/login", post(login))
        }
    "#;

    let model = parse_axum_file(source).expect("should parse");
    assert_eq!(model.endpoints.len(), 1);

    let ep = &model.endpoints[0];
    let form_ext = ep
        .handler
        .extractors
        .iter()
        .find(|e| e.kind == ExtractorKind::Form);
    assert!(form_ext.is_some(), "should detect Form extractor");

    let typeway_output = typeway_migrate::axum_to_typeway(source);
    assert!(typeway_output.is_ok(), "axum_to_typeway should succeed");
}

#[test]
fn websocket_upgrade_generates_warning() {
    let source = r#"
        use axum::{Router, routing::get, extract::ws::WebSocketUpgrade};

        async fn ws_handler(ws: WebSocketUpgrade) -> impl axum::response::IntoResponse {
            ws.on_upgrade(|socket| async { })
        }

        fn app() -> Router {
            Router::new()
                .route("/ws", get(ws_handler))
        }
    "#;

    let model = parse_axum_file(source).expect("should parse");
    assert_eq!(model.endpoints.len(), 1);

    let ep = &model.endpoints[0];
    let ws_ext = ep
        .handler
        .extractors
        .iter()
        .find(|e| e.kind == ExtractorKind::WebSocketUpgrade);
    assert!(ws_ext.is_some(), "should detect WebSocketUpgrade extractor");

    // Check that a WebSocket warning was generated.
    let has_ws_warning = model
        .warnings
        .iter()
        .any(|w| w.contains("WebSocket") && w.contains("ws_handler"));
    assert!(has_ws_warning, "should have WebSocket warning, got: {:?}", model.warnings);
}
