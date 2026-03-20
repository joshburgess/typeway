//! Simple `.proto` file parser for proto3 syntax.
//!
//! Extracts syntax, package, service/rpc definitions, and message definitions.
//! This is intentionally minimal — no support for imports, enums, oneof,
//! or options. Just the core proto3 subset needed for typeway interop,
//! including `map<K, V>` fields.

/// A parsed `.proto` file.
#[derive(Debug, Clone)]
pub struct ProtoFile {
    /// The syntax version (e.g., `"proto3"`).
    pub syntax: String,
    /// The package name (e.g., `"users.v1"`).
    pub package: String,
    /// Service definitions.
    pub services: Vec<ProtoService>,
    /// Message definitions.
    pub messages: Vec<ParsedMessage>,
}

/// A gRPC service definition.
#[derive(Debug, Clone)]
pub struct ProtoService {
    /// Service name (PascalCase).
    pub name: String,
    /// RPC methods defined in the service.
    pub methods: Vec<ProtoRpcMethod>,
}

/// A single RPC method in a service.
#[derive(Debug, Clone)]
pub struct ProtoRpcMethod {
    /// Method name (PascalCase).
    pub name: String,
    /// Input message type name.
    pub input_type: String,
    /// Output message type name.
    pub output_type: String,
    /// Comment preceding the rpc line (e.g., `"// GET /users/{id}"`).
    pub comment: Option<String>,
}

/// A parsed message definition.
#[derive(Debug, Clone)]
pub struct ParsedMessage {
    /// Message name (PascalCase).
    pub name: String,
    /// Fields in the message.
    pub fields: Vec<ParsedField>,
}

/// A single field in a message.
#[derive(Debug, Clone)]
pub struct ParsedField {
    /// Field name (snake_case).
    pub name: String,
    /// Protobuf type (e.g., `"string"`, `"uint32"`, `"User"`).
    /// For map fields, this is the full `map<K, V>` representation.
    pub proto_type: String,
    /// Field tag number.
    pub tag: u32,
    /// Whether the field is `repeated`.
    pub repeated: bool,
    /// Whether the field is `optional`.
    pub optional: bool,
    /// Whether the field is a `map<K, V>` type.
    pub is_map: bool,
    /// The key type for map fields (e.g., `"string"`).
    pub map_key_type: Option<String>,
    /// The value type for map fields (e.g., `"uint32"`).
    pub map_value_type: Option<String>,
}

/// Parse a `.proto` file string into a [`ProtoFile`].
///
/// This is a line-by-line parser that handles a minimal proto3 subset:
/// - `syntax = "proto3";`
/// - `package foo.bar.v1;`
/// - `service Name { ... }` blocks with `rpc` methods
/// - `message Name { ... }` blocks with fields
/// - Single-line `// ...` comments (attached to the next rpc/message item)
/// - Nested messages (flattened with dotted names)
///
/// # Errors
///
/// Returns a description of the parse error.
pub fn parse_proto(source: &str) -> Result<ProtoFile, String> {
    let mut syntax = String::new();
    let mut package = String::new();
    let mut services = Vec::new();
    let mut messages = Vec::new();

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Skip empty lines and standalone comments at the top level.
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // syntax = "proto3";
        if trimmed.starts_with("syntax") {
            syntax = parse_syntax(trimmed)?;
            i += 1;
            continue;
        }

        // package foo.bar.v1;
        if trimmed.starts_with("package") {
            package = parse_package(trimmed)?;
            i += 1;
            continue;
        }

        // service Name { ... }
        if trimmed.starts_with("service") {
            let (svc, next) = parse_service(&lines, i)?;
            services.push(svc);
            i = next;
            continue;
        }

        // message Name { ... }
        if trimmed.starts_with("message") {
            let (msg_list, next) = parse_message_block(&lines, i, "")?;
            messages.extend(msg_list);
            i = next;
            continue;
        }

        // Skip comments and unknown lines at the top level.
        i += 1;
    }

    Ok(ProtoFile {
        syntax,
        package,
        services,
        messages,
    })
}

/// Parse `syntax = "proto3";` line.
fn parse_syntax(line: &str) -> Result<String, String> {
    // syntax = "proto3";
    let rest = line
        .trim_start_matches("syntax")
        .trim()
        .trim_start_matches('=')
        .trim()
        .trim_end_matches(';')
        .trim()
        .trim_matches('"');
    if rest.is_empty() {
        return Err("empty syntax declaration".to_string());
    }
    Ok(rest.to_string())
}

/// Parse `package foo.bar.v1;` line.
fn parse_package(line: &str) -> Result<String, String> {
    let rest = line
        .trim_start_matches("package")
        .trim()
        .trim_end_matches(';')
        .trim();
    if rest.is_empty() {
        return Err("empty package declaration".to_string());
    }
    Ok(rest.to_string())
}

/// Parse a `service Name { ... }` block starting at line `start`.
///
/// Returns the parsed service and the index of the line after the closing `}`.
fn parse_service(lines: &[&str], start: usize) -> Result<(ProtoService, usize), String> {
    let header = lines[start].trim();
    let name = extract_block_name(header, "service")?;

    let mut methods = Vec::new();
    let mut i = start + 1;
    let mut pending_comment: Option<String> = None;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed == "}" {
            return Ok((ProtoService { name, methods }, i + 1));
        }

        // Collect comments to attach to the next rpc.
        if trimmed.starts_with("//") {
            pending_comment = Some(trimmed.to_string());
            i += 1;
            continue;
        }

        if trimmed.starts_with("rpc") {
            let mut method = parse_rpc(trimmed)?;
            method.comment = pending_comment.take();
            methods.push(method);
            i += 1;
            continue;
        }

        // Skip empty lines and unknown content inside service block.
        if trimmed.is_empty() {
            // Don't clear pending_comment on blank lines between comment and rpc.
            i += 1;
            continue;
        }

        pending_comment = None;
        i += 1;
    }

    Err(format!("unclosed service block '{}'", name))
}

/// Parse an `rpc MethodName(InputType) returns (OutputType);` line.
fn parse_rpc(line: &str) -> Result<ProtoRpcMethod, String> {
    let rest = line.trim_start_matches("rpc").trim();

    // Find method name (before the first '(').
    let paren_pos = rest
        .find('(')
        .ok_or_else(|| format!("missing '(' in rpc: {}", line))?;
    let name = rest[..paren_pos].trim().to_string();

    // Find input type between first '(' and first ')'.
    let after_open = &rest[paren_pos + 1..];
    let close_pos = after_open
        .find(')')
        .ok_or_else(|| format!("missing ')' in rpc: {}", line))?;
    let input_type = after_open[..close_pos].trim().to_string();

    // Find "returns" keyword.
    let after_input = &after_open[close_pos + 1..];
    let returns_pos = after_input
        .find("returns")
        .ok_or_else(|| format!("missing 'returns' in rpc: {}", line))?;
    let after_returns = &after_input[returns_pos + "returns".len()..];

    // Find output type between '(' and ')'.
    let out_open = after_returns
        .find('(')
        .ok_or_else(|| format!("missing '(' after returns in rpc: {}", line))?;
    let after_out_open = &after_returns[out_open + 1..];
    let out_close = after_out_open
        .find(')')
        .ok_or_else(|| format!("missing ')' after returns in rpc: {}", line))?;
    let output_type = after_out_open[..out_close].trim().to_string();

    Ok(ProtoRpcMethod {
        name,
        input_type,
        output_type,
        comment: None,
    })
}

/// Parse a `message Name { ... }` block, flattening nested messages.
///
/// `prefix` is used for nested messages (e.g., `"Outer."` for a nested message
/// inside `Outer`). Returns all messages (including nested) and the index of
/// the line after the closing `}`.
fn parse_message_block(
    lines: &[&str],
    start: usize,
    prefix: &str,
) -> Result<(Vec<ParsedMessage>, usize), String> {
    let header = lines[start].trim();
    let raw_name = extract_block_name(header, "message")?;
    let full_name = if prefix.is_empty() {
        raw_name.clone()
    } else {
        format!("{}{}", prefix, raw_name)
    };

    let mut fields = Vec::new();
    let mut nested = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed == "}" {
            let mut result = vec![ParsedMessage {
                name: full_name,
                fields,
            }];
            result.extend(nested);
            return Ok((result, i + 1));
        }

        // Nested message.
        if trimmed.starts_with("message") {
            let nested_prefix = format!("{}.", full_name);
            let (msgs, next) = parse_message_block(lines, i, &nested_prefix)?;
            nested.extend(msgs);
            i = next;
            continue;
        }

        // Skip comments and empty lines inside message blocks.
        if trimmed.is_empty() || trimmed.starts_with("//") {
            i += 1;
            continue;
        }

        // Try to parse as a field.
        if let Some(field) = parse_field(trimmed) {
            fields.push(field);
        }

        i += 1;
    }

    Err(format!("unclosed message block '{}'", raw_name))
}

/// Parse a field line like `string name = 1;`, `repeated uint32 ids = 2;`,
/// or `map<string, uint32> metadata = 3;`.
///
/// Returns `None` if the line doesn't look like a field.
fn parse_field(line: &str) -> Option<ParsedField> {
    // Strip trailing comment.
    let line = line.split("//").next().unwrap_or(line).trim();
    // Strip trailing semicolon.
    let line = line.trim_end_matches(';').trim();

    // Check for map<K, V> syntax.
    if line.starts_with("map<") || line.starts_with("map <") {
        return parse_map_field(line);
    }

    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 4 {
        return None;
    }

    let mut idx = 0;
    let mut repeated = false;
    let mut optional = false;

    // Check for repeated/optional prefix.
    if tokens[idx] == "repeated" {
        repeated = true;
        idx += 1;
    } else if tokens[idx] == "optional" {
        optional = true;
        idx += 1;
    }

    if idx + 3 > tokens.len() {
        return None;
    }

    let proto_type = tokens[idx].to_string();
    idx += 1;
    let name = tokens[idx].to_string();
    idx += 1;

    // Expect '=' sign.
    if tokens[idx] != "=" {
        return None;
    }
    idx += 1;

    if idx >= tokens.len() {
        return None;
    }

    let tag: u32 = tokens[idx].parse().ok()?;

    Some(ParsedField {
        name,
        proto_type,
        tag,
        repeated,
        optional,
        is_map: false,
        map_key_type: None,
        map_value_type: None,
    })
}

/// Parse a `map<K, V> name = tag;` field line.
fn parse_map_field(line: &str) -> Option<ParsedField> {
    // Find the angle bracket contents: map<K, V>
    let open = line.find('<')?;
    let close = line.find('>')?;
    if close <= open + 1 {
        return None;
    }

    let inner = &line[open + 1..close];
    let comma = inner.find(',')?;
    let key_type = inner[..comma].trim().to_string();
    let value_type = inner[comma + 1..].trim().to_string();

    // After the '>', parse: name = tag
    let rest = line[close + 1..].trim();
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.len() < 3 {
        return None;
    }

    let name = tokens[0].to_string();
    if tokens[1] != "=" {
        return None;
    }
    let tag: u32 = tokens[2].parse().ok()?;

    let proto_type = format!("map<{}, {}>", key_type, value_type);

    Some(ParsedField {
        name,
        proto_type,
        tag,
        repeated: false,
        optional: false,
        is_map: true,
        map_key_type: Some(key_type),
        map_value_type: Some(value_type),
    })
}

/// Extract the name from a block header like `service Foo {` or `message Bar {`.
fn extract_block_name(header: &str, keyword: &str) -> Result<String, String> {
    let rest = header.trim_start_matches(keyword).trim();
    // Strip trailing '{' and whitespace.
    let name = rest.trim_end_matches('{').trim();
    if name.is_empty() {
        return Err(format!("missing name after '{}'", keyword));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_PROTO: &str = r#"syntax = "proto3";

package users.v1;

service UserService {
  // GET /users
  rpc ListUser(google.protobuf.Empty) returns (ListUserResponse);
  // GET /users/{}
  rpc GetUser(GetUserRequest) returns (User);
  // POST /users
  rpc CreateUser(CreateUserRequest) returns (User);
  // DELETE /users/{}
  rpc DeleteUser(DeleteUserRequest) returns (google.protobuf.Empty);
}

message User {
  uint32 id = 1;
  string name = 2;
  string email = 3;
}

message GetUserRequest {
  string param1 = 1;
}

message CreateUserRequest {
  string name = 1;
  string email = 2;
}

message DeleteUserRequest {
  string param1 = 1;
}

message ListUserResponse {
  repeated User users = 1;
}
"#;

    #[test]
    fn parses_syntax() {
        let proto = parse_proto(SAMPLE_PROTO).unwrap();
        assert_eq!(proto.syntax, "proto3");
    }

    #[test]
    fn parses_package() {
        let proto = parse_proto(SAMPLE_PROTO).unwrap();
        assert_eq!(proto.package, "users.v1");
    }

    #[test]
    fn parses_service() {
        let proto = parse_proto(SAMPLE_PROTO).unwrap();
        assert_eq!(proto.services.len(), 1);
        assert_eq!(proto.services[0].name, "UserService");
        assert_eq!(proto.services[0].methods.len(), 4);
    }

    #[test]
    fn parses_rpc_methods() {
        let proto = parse_proto(SAMPLE_PROTO).unwrap();
        let methods = &proto.services[0].methods;

        assert_eq!(methods[0].name, "ListUser");
        assert_eq!(methods[0].input_type, "google.protobuf.Empty");
        assert_eq!(methods[0].output_type, "ListUserResponse");

        assert_eq!(methods[1].name, "GetUser");
        assert_eq!(methods[1].input_type, "GetUserRequest");
        assert_eq!(methods[1].output_type, "User");

        assert_eq!(methods[2].name, "CreateUser");
        assert_eq!(methods[2].input_type, "CreateUserRequest");
        assert_eq!(methods[2].output_type, "User");

        assert_eq!(methods[3].name, "DeleteUser");
        assert_eq!(methods[3].input_type, "DeleteUserRequest");
        assert_eq!(methods[3].output_type, "google.protobuf.Empty");
    }

    #[test]
    fn parses_rpc_comments() {
        let proto = parse_proto(SAMPLE_PROTO).unwrap();
        let methods = &proto.services[0].methods;

        assert_eq!(methods[0].comment.as_deref(), Some("// GET /users"));
        assert_eq!(methods[1].comment.as_deref(), Some("// GET /users/{}"));
        assert_eq!(methods[2].comment.as_deref(), Some("// POST /users"));
        assert_eq!(methods[3].comment.as_deref(), Some("// DELETE /users/{}"));
    }

    #[test]
    fn parses_messages() {
        let proto = parse_proto(SAMPLE_PROTO).unwrap();
        assert_eq!(proto.messages.len(), 5);

        let names: Vec<&str> = proto.messages.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"User"));
        assert!(names.contains(&"GetUserRequest"));
        assert!(names.contains(&"CreateUserRequest"));
        assert!(names.contains(&"DeleteUserRequest"));
        assert!(names.contains(&"ListUserResponse"));
    }

    #[test]
    fn parses_field_types() {
        let proto = parse_proto(SAMPLE_PROTO).unwrap();
        let user = proto.messages.iter().find(|m| m.name == "User").unwrap();
        assert_eq!(user.fields.len(), 3);
        assert_eq!(user.fields[0].proto_type, "uint32");
        assert_eq!(user.fields[0].name, "id");
        assert_eq!(user.fields[0].tag, 1);
        assert_eq!(user.fields[1].proto_type, "string");
        assert_eq!(user.fields[1].name, "name");
        assert_eq!(user.fields[2].proto_type, "string");
        assert_eq!(user.fields[2].name, "email");
    }

    #[test]
    fn parses_repeated_fields() {
        let proto = parse_proto(SAMPLE_PROTO).unwrap();
        let list_resp = proto
            .messages
            .iter()
            .find(|m| m.name == "ListUserResponse")
            .unwrap();
        assert_eq!(list_resp.fields.len(), 1);
        assert!(list_resp.fields[0].repeated);
        assert_eq!(list_resp.fields[0].proto_type, "User");
        assert_eq!(list_resp.fields[0].name, "users");
    }

    #[test]
    fn parses_optional_fields() {
        let source = r#"syntax = "proto3";
package test.v1;
message Foo {
  optional bytes data = 1;
  string name = 2;
}
"#;
        let proto = parse_proto(source).unwrap();
        let foo = &proto.messages[0];
        assert!(foo.fields[0].optional);
        assert!(!foo.fields[1].optional);
    }

    #[test]
    fn nested_messages_flattened() {
        let source = r#"syntax = "proto3";
package test.v1;
message Outer {
  string name = 1;
  message Inner {
    uint32 id = 1;
  }
}
"#;
        let proto = parse_proto(source).unwrap();
        assert_eq!(proto.messages.len(), 2);
        assert_eq!(proto.messages[0].name, "Outer");
        assert_eq!(proto.messages[1].name, "Outer.Inner");
    }

    #[test]
    fn empty_proto() {
        let source = r#"syntax = "proto3";
package empty.v1;
"#;
        let proto = parse_proto(source).unwrap();
        assert_eq!(proto.syntax, "proto3");
        assert_eq!(proto.package, "empty.v1");
        assert!(proto.services.is_empty());
        assert!(proto.messages.is_empty());
    }

    #[test]
    fn error_on_unclosed_service() {
        let source = r#"syntax = "proto3";
package test.v1;
service Broken {
  rpc Foo(A) returns (B);
"#;
        let result = parse_proto(source);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unclosed service"));
    }

    #[test]
    fn error_on_unclosed_message() {
        let source = r#"syntax = "proto3";
package test.v1;
message Broken {
  string name = 1;
"#;
        let result = parse_proto(source);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unclosed message"));
    }

    #[test]
    fn parses_map_fields() {
        let source = r#"syntax = "proto3";
package test.v1;
message Config {
  string name = 1;
  map<string, string> metadata = 2;
  map<string, uint32> counts = 3;
}
"#;
        let proto = parse_proto(source).unwrap();
        let config = &proto.messages[0];
        assert_eq!(config.fields.len(), 3);

        // Regular field.
        assert_eq!(config.fields[0].name, "name");
        assert!(!config.fields[0].is_map);

        // Map fields.
        assert_eq!(config.fields[1].name, "metadata");
        assert!(config.fields[1].is_map);
        assert_eq!(config.fields[1].map_key_type.as_deref(), Some("string"));
        assert_eq!(config.fields[1].map_value_type.as_deref(), Some("string"));
        assert_eq!(config.fields[1].tag, 2);
        assert_eq!(config.fields[1].proto_type, "map<string, string>");

        assert_eq!(config.fields[2].name, "counts");
        assert!(config.fields[2].is_map);
        assert_eq!(config.fields[2].map_key_type.as_deref(), Some("string"));
        assert_eq!(config.fields[2].map_value_type.as_deref(), Some("uint32"));
        assert_eq!(config.fields[2].tag, 3);
    }

    #[test]
    fn map_fields_are_not_repeated_or_optional() {
        let source = r#"syntax = "proto3";
package test.v1;
message Foo {
  map<uint32, string> items = 1;
}
"#;
        let proto = parse_proto(source).unwrap();
        let field = &proto.messages[0].fields[0];
        assert!(field.is_map);
        assert!(!field.repeated);
        assert!(!field.optional);
    }
}
