//! Proto file validation.
//!
//! [`validate_proto`] parses a `.proto` file string and checks for common
//! issues: duplicate field tags, invalid tag ranges, reserved words used as
//! field names, invalid proto types, and undefined RPC message types.

use std::collections::{HashMap, HashSet};

use crate::proto_parse::{self, ParsedMessage, ProtoFile};

/// A validation error found in a proto file.
#[derive(Debug, Clone)]
pub struct ProtoValidationError {
    /// The message or service name where the error was found, if applicable.
    pub message_name: Option<String>,
    /// The field or method name where the error was found, if applicable.
    pub field_name: Option<String>,
    /// Human-readable description of the error.
    pub error: String,
}

impl std::fmt::Display for ProtoValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.message_name, &self.field_name) {
            (Some(msg), Some(field)) => write!(f, "{}.{}: {}", msg, field, self.error),
            (Some(msg), None) => write!(f, "{}: {}", msg, self.error),
            _ => write!(f, "{}", self.error),
        }
    }
}

/// Validate a generated `.proto` file string for common issues.
///
/// Returns a list of validation errors. An empty list means the proto is valid.
pub fn validate_proto(proto: &str) -> Vec<ProtoValidationError> {
    let mut errors = Vec::new();

    match proto_parse::parse_proto(proto) {
        Ok(file) => validate_proto_file(&file, &mut errors),
        Err(e) => errors.push(ProtoValidationError {
            message_name: None,
            field_name: None,
            error: format!("parse error: {}", e),
        }),
    }

    errors
}

fn validate_proto_file(file: &ProtoFile, errors: &mut Vec<ProtoValidationError>) {
    // Check syntax.
    if file.syntax != "proto3" {
        errors.push(ProtoValidationError {
            message_name: None,
            field_name: None,
            error: format!("expected syntax \"proto3\", got \"{}\"", file.syntax),
        });
    }

    // Validate each message.
    for msg in &file.messages {
        validate_message(msg, errors);
    }

    // Validate service methods reference existing messages.
    let message_names: HashSet<&str> = file.messages.iter().map(|m| m.name.as_str()).collect();

    for service in &file.services {
        for method in &service.methods {
            // Strip "stream " prefix if present (parser may include it).
            let input = method.input_type.strip_prefix("stream ").unwrap_or(&method.input_type);
            let output = method.output_type.strip_prefix("stream ").unwrap_or(&method.output_type);

            if input != "google.protobuf.Empty" && !message_names.contains(input) {
                errors.push(ProtoValidationError {
                    message_name: Some(service.name.clone()),
                    field_name: Some(method.name.clone()),
                    error: format!("RPC input type '{}' not defined", input),
                });
            }
            if output != "google.protobuf.Empty" && !message_names.contains(output) {
                errors.push(ProtoValidationError {
                    message_name: Some(service.name.clone()),
                    field_name: Some(method.name.clone()),
                    error: format!("RPC output type '{}' not defined", output),
                });
            }
        }
    }
}

/// Proto reserved words that cannot be used as field names.
const PROTO_RESERVED_WORDS: &[&str] = &[
    "syntax", "import", "package", "option", "message", "enum", "service", "rpc", "returns",
    "stream", "repeated", "optional", "map", "oneof", "reserved", "extensions", "extend",
];

/// Valid protobuf scalar type names.
const VALID_SCALARS: &[&str] = &[
    "double", "float", "int32", "int64", "uint32", "uint64", "sint32", "sint64", "fixed32",
    "fixed64", "sfixed32", "sfixed64", "bool", "string", "bytes",
];

fn validate_message(msg: &ParsedMessage, errors: &mut Vec<ProtoValidationError>) {
    let mut seen_tags: HashMap<u32, String> = HashMap::new();

    for field in &msg.fields {
        // Check tag uniqueness.
        if let Some(existing) = seen_tags.get(&field.tag) {
            errors.push(ProtoValidationError {
                message_name: Some(msg.name.clone()),
                field_name: Some(field.name.clone()),
                error: format!(
                    "duplicate tag {}: already used by field '{}'",
                    field.tag, existing
                ),
            });
        } else {
            seen_tags.insert(field.tag, field.name.clone());
        }

        // Check tag range (1 to 536870911, excluding 19000-19999).
        if field.tag == 0 {
            errors.push(ProtoValidationError {
                message_name: Some(msg.name.clone()),
                field_name: Some(field.name.clone()),
                error: "tag must be >= 1".to_string(),
            });
        }
        if field.tag > 536_870_911 {
            errors.push(ProtoValidationError {
                message_name: Some(msg.name.clone()),
                field_name: Some(field.name.clone()),
                error: format!("tag {} exceeds maximum (536870911)", field.tag),
            });
        }
        if (19000..=19999).contains(&field.tag) {
            errors.push(ProtoValidationError {
                message_name: Some(msg.name.clone()),
                field_name: Some(field.name.clone()),
                error: format!("tag {} is in the reserved range 19000-19999", field.tag),
            });
        }

        // Check for proto reserved words as field names.
        if PROTO_RESERVED_WORDS.contains(&field.name.as_str()) {
            errors.push(ProtoValidationError {
                message_name: Some(msg.name.clone()),
                field_name: Some(field.name.clone()),
                error: format!("'{}' is a proto reserved word", field.name),
            });
        }

        // Check valid proto type names. Skip map fields — they use map<K, V> syntax.
        if !field.is_map
            && !VALID_SCALARS.contains(&field.proto_type.as_str())
            && !field.proto_type.starts_with(char::is_uppercase)
            && field.proto_type != "google.protobuf.Empty"
        {
            errors.push(ProtoValidationError {
                message_name: Some(msg.name.clone()),
                field_name: Some(field.name.clone()),
                error: format!("'{}' is not a valid proto type", field.proto_type),
            });
        }
    }
}
