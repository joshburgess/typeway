//! HTML documentation page generator for gRPC services.
//!
//! Generates a self-contained HTML page from a [`GrpcServiceSpec`], providing
//! a browsable reference similar to Swagger UI but designed for gRPC services.
//!
//! The page is static (no JavaScript framework), uses inline CSS, and has no
//! external dependencies — suitable for offline use.
//!
//! # Example
//!
//! ```ignore
//! use typeway_grpc::spec::ApiToGrpcSpec;
//! use typeway_grpc::docs_page::generate_docs_html;
//!
//! let spec = MyAPI::grpc_spec("UserService", "users.v1");
//! let html = generate_docs_html(&spec);
//! std::fs::write("grpc-docs.html", html).unwrap();
//! ```

use crate::spec::{FieldSpec, GrpcServiceSpec, MethodSpec};

/// Generate a self-contained HTML documentation page for the gRPC service.
///
/// The page includes:
/// - Service name, package, and description
/// - A table of all RPC methods with streaming mode, request/response types,
///   and descriptions
/// - Expandable message definitions showing all fields
/// - The raw `.proto` file in a syntax-highlighted code block
pub fn generate_docs_html(spec: &GrpcServiceSpec) -> String {
    let mut html = String::with_capacity(8192);

    // HTML header with inline CSS
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    html.push_str(&format!(
        "<title>{} - gRPC Service Documentation</title>\n",
        escape_html(&spec.service.name)
    ));
    html.push_str("<style>\n");
    html.push_str(CSS);
    html.push_str("</style>\n</head>\n<body>\n");

    // Header
    html.push_str("<div class=\"container\">\n");
    html.push_str("<header>\n");
    html.push_str(&format!("<h1>{}</h1>\n", escape_html(&spec.service.name)));
    html.push_str(&format!(
        "<p class=\"package\">Package: <code>{}</code></p>\n",
        escape_html(&spec.service.package)
    ));
    html.push_str(&format!(
        "<p class=\"full-name\">Full name: <code>{}</code></p>\n",
        escape_html(&spec.service.full_name)
    ));
    if let Some(ref desc) = spec.service.description {
        html.push_str(&format!(
            "<p class=\"description\">{}</p>\n",
            escape_html(desc)
        ));
    }
    if let Some(ref version) = spec.service.version {
        html.push_str(&format!(
            "<p class=\"version\">Version: {}</p>\n",
            escape_html(version)
        ));
    }
    html.push_str("</header>\n\n");

    // Methods section
    html.push_str("<section id=\"methods\">\n");
    html.push_str("<h2>RPC Methods</h2>\n");

    if spec.methods.is_empty() {
        html.push_str("<p>No methods defined.</p>\n");
    } else {
        html.push_str("<table class=\"methods-table\">\n<thead>\n<tr>\n");
        html.push_str("<th>Method</th>\n");
        html.push_str("<th>REST Mapping</th>\n");
        html.push_str("<th>Request</th>\n");
        html.push_str("<th>Response</th>\n");
        html.push_str("<th>Streaming</th>\n");
        html.push_str("<th>Description</th>\n");
        html.push_str("</tr>\n</thead>\n<tbody>\n");

        for method in spec.methods.values() {
            render_method_row(&mut html, method);
        }

        html.push_str("</tbody>\n</table>\n");
    }
    html.push_str("</section>\n\n");

    // Method details
    for method in spec.methods.values() {
        render_method_detail(&mut html, method);
    }

    // Messages section
    if !spec.messages.is_empty() {
        html.push_str("<section id=\"messages\">\n");
        html.push_str("<h2>Message Definitions</h2>\n");

        for msg in spec.messages.values() {
            html.push_str(&format!(
                "<div class=\"message\" id=\"msg-{}\">\n",
                escape_html(&msg.name)
            ));
            html.push_str(&format!("<h3>{}</h3>\n", escape_html(&msg.name)));
            if let Some(ref desc) = msg.description {
                html.push_str(&format!("<p>{}</p>\n", escape_html(desc)));
            }

            if msg.fields.is_empty() {
                html.push_str("<p class=\"empty\">No fields (empty message).</p>\n");
            } else {
                html.push_str("<table class=\"fields-table\">\n<thead>\n<tr>\n");
                html.push_str("<th>Tag</th><th>Name</th><th>Type</th><th>Modifiers</th><th>Description</th>\n");
                html.push_str("</tr>\n</thead>\n<tbody>\n");

                for field in &msg.fields {
                    render_field_row(&mut html, field);
                }

                html.push_str("</tbody>\n</table>\n");
            }

            html.push_str("</div>\n");
        }

        html.push_str("</section>\n\n");
    }

    // Proto file section
    html.push_str("<section id=\"proto\">\n");
    html.push_str("<h2>Proto Definition</h2>\n");
    html.push_str("<pre class=\"proto-code\"><code>");
    html.push_str(&escape_html(&spec.proto));
    html.push_str("</code></pre>\n");
    html.push_str("</section>\n\n");

    // Footer
    html.push_str("<footer>\n");
    html.push_str("<p>Generated by <strong>typeway-grpc</strong></p>\n");
    html.push_str("</footer>\n");

    html.push_str("</div>\n</body>\n</html>\n");
    html
}

// ---------------------------------------------------------------------------
// Render helpers
// ---------------------------------------------------------------------------

/// Render a single method row in the methods summary table.
fn render_method_row(html: &mut String, method: &MethodSpec) {
    html.push_str("<tr>\n");
    html.push_str(&format!(
        "<td><a href=\"#method-{}\" class=\"method-link\">{}</a></td>\n",
        escape_html(&method.name),
        escape_html(&method.name),
    ));
    html.push_str(&format!(
        "<td><span class=\"http-method method-{}\">{}</span> <code>{}</code></td>\n",
        method.http_method.to_lowercase(),
        escape_html(&method.http_method),
        escape_html(&method.rest_path),
    ));
    html.push_str(&format!(
        "<td><code>{}</code></td>\n",
        escape_html(&method.request_type),
    ));
    html.push_str(&format!(
        "<td><code>{}</code></td>\n",
        escape_html(&method.response_type),
    ));
    html.push_str(&format!("<td>{}</td>\n", streaming_label(method)));
    html.push_str(&format!(
        "<td>{}</td>\n",
        method
            .summary
            .as_deref()
            .or(method.description.as_deref())
            .unwrap_or("-")
    ));
    html.push_str("</tr>\n");
}

/// Render a detailed method section with full gRPC path and docs.
fn render_method_detail(html: &mut String, method: &MethodSpec) {
    html.push_str(&format!(
        "<section class=\"method-detail\" id=\"method-{}\">\n",
        escape_html(&method.name)
    ));
    html.push_str(&format!("<h3>{}</h3>\n", escape_html(&method.name)));
    html.push_str("<dl>\n");

    html.push_str(&format!(
        "<dt>gRPC Path</dt><dd><code>{}</code></dd>\n",
        escape_html(&method.full_path)
    ));
    html.push_str(&format!(
        "<dt>REST Mapping</dt><dd><span class=\"http-method method-{}\">{}</span> <code>{}</code></dd>\n",
        method.http_method.to_lowercase(),
        escape_html(&method.http_method),
        escape_html(&method.rest_path),
    ));
    html.push_str(&format!(
        "<dt>Request</dt><dd><code>{}</code></dd>\n",
        escape_html(&method.request_type)
    ));
    html.push_str(&format!(
        "<dt>Response</dt><dd><code>{}</code></dd>\n",
        escape_html(&method.response_type)
    ));
    html.push_str(&format!(
        "<dt>Streaming</dt><dd>{}</dd>\n",
        streaming_label(method)
    ));

    if method.requires_auth {
        html.push_str("<dt>Authentication</dt><dd>Required</dd>\n");
    }

    if !method.tags.is_empty() {
        let tags_html: Vec<String> = method
            .tags
            .iter()
            .map(|t| format!("<span class=\"tag\">{}</span>", escape_html(t)))
            .collect();
        html.push_str(&format!("<dt>Tags</dt><dd>{}</dd>\n", tags_html.join(" ")));
    }

    html.push_str("</dl>\n");

    if let Some(ref summary) = method.summary {
        html.push_str(&format!(
            "<p class=\"summary\">{}</p>\n",
            escape_html(summary)
        ));
    }
    if let Some(ref desc) = method.description {
        html.push_str(&format!(
            "<p class=\"description\">{}</p>\n",
            escape_html(desc)
        ));
    }

    html.push_str("</section>\n\n");
}

/// Render a single field row in a message definition table.
fn render_field_row(html: &mut String, field: &FieldSpec) {
    html.push_str("<tr>\n");
    html.push_str(&format!("<td>{}</td>\n", field.tag));
    html.push_str(&format!(
        "<td><code>{}</code></td>\n",
        escape_html(&field.name)
    ));

    // Type column
    if field.is_map {
        let key = field.map_key_type.as_deref().unwrap_or("string");
        let value = field.map_value_type.as_deref().unwrap_or("string");
        html.push_str(&format!(
            "<td><code>map&lt;{}, {}&gt;</code></td>\n",
            escape_html(key),
            escape_html(value),
        ));
    } else {
        html.push_str(&format!(
            "<td><code>{}</code></td>\n",
            escape_html(&field.proto_type)
        ));
    }

    // Modifiers column
    let mut modifiers = Vec::new();
    if field.repeated {
        modifiers.push("repeated");
    }
    if field.optional {
        modifiers.push("optional");
    }
    html.push_str(&format!(
        "<td>{}</td>\n",
        if modifiers.is_empty() {
            "-".to_string()
        } else {
            modifiers.join(", ")
        }
    ));

    // Description column
    html.push_str(&format!(
        "<td>{}</td>\n",
        field.description.as_deref().unwrap_or("-")
    ));

    html.push_str("</tr>\n");
}

/// Return a human-readable streaming label for a method.
fn streaming_label(method: &MethodSpec) -> &'static str {
    match (method.client_streaming, method.server_streaming) {
        (true, true) => "Bidirectional",
        (true, false) => "Client",
        (false, true) => "Server",
        (false, false) => "Unary",
    }
}

/// Escape HTML special characters.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// Inline CSS
// ---------------------------------------------------------------------------

const CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
    line-height: 1.6;
    color: #1a1a2e;
    background: #f8f9fa;
}
.container {
    max-width: 960px;
    margin: 0 auto;
    padding: 2rem 1.5rem;
}
header {
    margin-bottom: 2rem;
    padding-bottom: 1rem;
    border-bottom: 2px solid #e0e0e0;
}
header h1 {
    font-size: 1.75rem;
    margin-bottom: 0.5rem;
}
header .package, header .full-name, header .version {
    color: #555;
    font-size: 0.95rem;
    margin-bottom: 0.25rem;
}
header .description {
    margin-top: 0.5rem;
    color: #333;
}
h2 {
    font-size: 1.35rem;
    margin: 1.5rem 0 1rem;
    padding-bottom: 0.3rem;
    border-bottom: 1px solid #e0e0e0;
}
h3 {
    font-size: 1.1rem;
    margin-bottom: 0.5rem;
}
code {
    background: #eef;
    padding: 0.15em 0.4em;
    border-radius: 3px;
    font-size: 0.9em;
    font-family: "SF Mono", Monaco, Consolas, "Liberation Mono", monospace;
}
table {
    width: 100%;
    border-collapse: collapse;
    margin-bottom: 1.5rem;
    font-size: 0.9rem;
}
th, td {
    text-align: left;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid #e0e0e0;
}
th {
    background: #f0f0f5;
    font-weight: 600;
}
tr:hover { background: #f5f5ff; }
.method-link {
    color: #2563eb;
    text-decoration: none;
    font-weight: 500;
}
.method-link:hover { text-decoration: underline; }
.http-method {
    display: inline-block;
    padding: 0.1em 0.5em;
    border-radius: 3px;
    font-size: 0.8em;
    font-weight: 600;
    color: #fff;
}
.method-get { background: #22c55e; }
.method-post { background: #3b82f6; }
.method-put { background: #f59e0b; }
.method-delete { background: #ef4444; }
.method-patch { background: #a855f7; }
.method-head { background: #64748b; }
.method-options { background: #64748b; }
.method-detail {
    background: #fff;
    border: 1px solid #e0e0e0;
    border-radius: 6px;
    padding: 1rem 1.25rem;
    margin-bottom: 1rem;
}
.method-detail dl {
    display: grid;
    grid-template-columns: 140px 1fr;
    gap: 0.3rem 1rem;
    margin-bottom: 0.75rem;
}
.method-detail dt { font-weight: 600; color: #555; }
.method-detail dd { color: #1a1a2e; }
.tag {
    display: inline-block;
    background: #dbeafe;
    color: #1e40af;
    padding: 0.1em 0.5em;
    border-radius: 3px;
    font-size: 0.85em;
    margin-right: 0.25rem;
}
.summary { font-weight: 500; margin-bottom: 0.25rem; }
.description { color: #444; }
.message {
    background: #fff;
    border: 1px solid #e0e0e0;
    border-radius: 6px;
    padding: 1rem 1.25rem;
    margin-bottom: 1rem;
}
.fields-table th { font-size: 0.85rem; }
.empty { color: #888; font-style: italic; }
.proto-code {
    background: #1e1e2e;
    color: #cdd6f4;
    padding: 1rem 1.25rem;
    border-radius: 6px;
    overflow-x: auto;
    font-size: 0.85rem;
    line-height: 1.5;
}
.proto-code code {
    background: none;
    padding: 0;
    color: inherit;
    font-size: inherit;
}
footer {
    margin-top: 2rem;
    padding-top: 1rem;
    border-top: 1px solid #e0e0e0;
    color: #888;
    font-size: 0.85rem;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::*;
    use indexmap::IndexMap;

    fn sample_spec() -> GrpcServiceSpec {
        let mut methods = IndexMap::new();
        methods.insert(
            "ListUser".to_string(),
            MethodSpec {
                name: "ListUser".to_string(),
                full_path: "/users.v1.UserService/ListUser".to_string(),
                rest_path: "/users".to_string(),
                http_method: "GET".to_string(),
                request_type: "google.protobuf.Empty".to_string(),
                response_type: "ListUserResponse".to_string(),
                server_streaming: false,
                client_streaming: false,
                description: Some("List all users".to_string()),
                summary: Some("List users".to_string()),
                tags: vec!["users".to_string()],
                requires_auth: false,
            },
        );
        methods.insert(
            "GetUser".to_string(),
            MethodSpec {
                name: "GetUser".to_string(),
                full_path: "/users.v1.UserService/GetUser".to_string(),
                rest_path: "/users/{}".to_string(),
                http_method: "GET".to_string(),
                request_type: "GetUserRequest".to_string(),
                response_type: "User".to_string(),
                server_streaming: false,
                client_streaming: false,
                description: None,
                summary: None,
                tags: Vec::new(),
                requires_auth: true,
            },
        );

        let mut messages = IndexMap::new();
        messages.insert(
            "User".to_string(),
            MessageSpec {
                name: "User".to_string(),
                fields: vec![
                    FieldSpec {
                        name: "id".to_string(),
                        proto_type: "uint32".to_string(),
                        tag: 1,
                        repeated: false,
                        optional: false,
                        is_map: false,
                        map_key_type: None,
                        map_value_type: None,
                        description: None,
                    },
                    FieldSpec {
                        name: "name".to_string(),
                        proto_type: "string".to_string(),
                        tag: 2,
                        repeated: false,
                        optional: false,
                        is_map: false,
                        map_key_type: None,
                        map_value_type: None,
                        description: Some("User display name".to_string()),
                    },
                ],
                description: None,
            },
        );

        GrpcServiceSpec {
            proto: "syntax = \"proto3\";\npackage users.v1;\nservice UserService {}".to_string(),
            service: ServiceInfo {
                name: "UserService".to_string(),
                package: "users.v1".to_string(),
                full_name: "users.v1.UserService".to_string(),
                description: Some("Manages user accounts".to_string()),
                version: Some("1.0".to_string()),
            },
            methods,
            messages,
        }
    }

    #[test]
    fn html_contains_service_name() {
        let html = generate_docs_html(&sample_spec());
        assert!(html.contains("UserService"));
    }

    #[test]
    fn html_contains_package() {
        let html = generate_docs_html(&sample_spec());
        assert!(html.contains("users.v1"));
    }

    #[test]
    fn html_contains_methods() {
        let html = generate_docs_html(&sample_spec());
        assert!(html.contains("ListUser"));
        assert!(html.contains("GetUser"));
    }

    #[test]
    fn html_contains_proto() {
        let html = generate_docs_html(&sample_spec());
        assert!(html.contains("proto3"));
        assert!(html.contains("service UserService"));
    }

    #[test]
    fn html_contains_message_fields() {
        let html = generate_docs_html(&sample_spec());
        assert!(html.contains("uint32"));
        assert!(html.contains("User display name"));
    }

    #[test]
    fn html_contains_description() {
        let html = generate_docs_html(&sample_spec());
        assert!(html.contains("Manages user accounts"));
    }

    #[test]
    fn html_contains_streaming_labels() {
        let html = generate_docs_html(&sample_spec());
        assert!(html.contains("Unary"));
    }

    #[test]
    fn html_is_valid_structure() {
        let html = generate_docs_html(&sample_spec());
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<html"));
        assert!(html.contains("</html>"));
        assert!(html.contains("<head>"));
        assert!(html.contains("</head>"));
        assert!(html.contains("<body>"));
        assert!(html.contains("</body>"));
    }

    #[test]
    fn html_escapes_special_chars() {
        let mut spec = sample_spec();
        spec.service.description = Some("A <b>bold</b> & \"quoted\" service".to_string());
        let html = generate_docs_html(&spec);
        assert!(html.contains("&lt;b&gt;bold&lt;/b&gt;"));
        assert!(html.contains("&amp;"));
        assert!(html.contains("&quot;quoted&quot;"));
    }

    #[test]
    fn empty_spec_produces_valid_html() {
        let spec = GrpcServiceSpec {
            proto: String::new(),
            service: ServiceInfo {
                name: "EmptyService".to_string(),
                package: "empty.v1".to_string(),
                full_name: "empty.v1.EmptyService".to_string(),
                description: None,
                version: None,
            },
            methods: IndexMap::new(),
            messages: IndexMap::new(),
        };
        let html = generate_docs_html(&spec);
        assert!(html.contains("EmptyService"));
        assert!(html.contains("No methods defined"));
    }
}
