//! Shared utilities for OpenAPI → Rust codegen (used by both v2 and v3).

/// Map an OpenAPI type + format to a Rust type name.
pub fn openapi_type_to_rust(ty: &str, format: Option<&str>) -> String {
    match (ty, format) {
        ("string", Some("date-time")) => "String".to_string(),
        ("string", Some("date")) => "String".to_string(),
        ("string", Some("uuid")) => "String".to_string(),
        ("string", Some("binary")) => "Vec<u8>".to_string(),
        ("string", Some("byte")) => "Vec<u8>".to_string(),
        ("string", _) => "String".to_string(),
        ("integer", Some("int32")) => "i32".to_string(),
        ("integer", Some("int64")) => "i64".to_string(),
        ("integer", _) => "i64".to_string(),
        ("number", Some("float")) => "f32".to_string(),
        ("number", Some("double")) => "f64".to_string(),
        ("number", _) => "f64".to_string(),
        ("boolean", _) => "bool".to_string(),
        ("array", _) => "Vec<serde_json::Value>".to_string(),
        ("object", _) => "serde_json::Value".to_string(),
        _ => "serde_json::Value".to_string(),
    }
}

/// Convert an OpenAPI path like `/users/{id}/posts` to typeway_path! args.
///
/// `/users/{id}` → `"users" / String`
/// `/users/{id}/posts` → `"users" / String / "posts"`
pub fn path_to_macro_args(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return "\"\"".to_string();
    }
    let parts: Vec<String> = segments
        .iter()
        .map(|seg| {
            if seg.starts_with('{') && seg.ends_with('}') {
                "String".to_string()
            } else {
                format!("\"{}\"", seg)
            }
        })
        .collect();
    parts.join(" / ")
}

/// Convert an OpenAPI path to a PascalCase type name.
///
/// `/users` → `UsersPath`
/// `/users/{id}` → `UsersByIdPath`
/// `/users/{id}/posts` → `UsersByIdPostsPath`
pub fn path_to_type_name(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return "RootPath".to_string();
    }
    let mut parts = Vec::new();
    for seg in &segments {
        if seg.starts_with('{') {
            parts.push("ById".to_string());
        } else {
            parts.push(capitalize(seg));
        }
    }
    format!("{}Path", parts.join(""))
}

/// Capitalize the first letter of a string.
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => {
            let upper: String = c.to_uppercase().collect();
            upper + &chars.collect::<String>()
        }
        None => String::new(),
    }
}

/// Convert a schema name to a valid Rust struct name.
pub fn sanitize_name(name: &str) -> String {
    name.replace(['.', '-', ' ', '/'], "_")
}

/// Extract the schema name from a `$ref` string.
///
/// `#/definitions/User` → `User` (v2)
/// `#/components/schemas/User` → `User` (v3)
pub fn ref_to_name(ref_str: &str) -> String {
    ref_str.rsplit('/').next().unwrap_or(ref_str).to_string()
}

/// Generate the endpoint type string for a path + method.
pub fn endpoint_type(
    method: &str,
    path_type: &str,
    req_type: Option<&str>,
    res_type: &str,
) -> String {
    match method.to_uppercase().as_str() {
        "GET" | "HEAD" => format!("GetEndpoint<{}, Json<{}>>", path_type, res_type),
        "DELETE" => format!("DeleteEndpoint<{}, Json<{}>>", path_type, res_type),
        "POST" => {
            let req = req_type.unwrap_or("serde_json::Value");
            format!(
                "PostEndpoint<{}, Json<{}>, Json<{}>>",
                path_type, req, res_type
            )
        }
        "PUT" => {
            let req = req_type.unwrap_or("serde_json::Value");
            format!(
                "PutEndpoint<{}, Json<{}>, Json<{}>>",
                path_type, req, res_type
            )
        }
        "PATCH" => {
            let req = req_type.unwrap_or("serde_json::Value");
            format!(
                "PatchEndpoint<{}, Json<{}>, Json<{}>>",
                path_type, req, res_type
            )
        }
        _ => format!("GetEndpoint<{}, Json<{}>>", path_type, res_type),
    }
}

/// Generate a Rust struct from a name and a list of (field_name, rust_type) pairs.
pub fn generate_struct(name: &str, fields: &[(String, String)]) -> String {
    let mut s = String::new();
    s.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
    s.push_str(&format!("pub struct {} {{\n", sanitize_name(name)));
    for (field_name, rust_type) in fields {
        s.push_str(&format!("    pub {}: {},\n", field_name, rust_type));
    }
    s.push('}');
    s
}
