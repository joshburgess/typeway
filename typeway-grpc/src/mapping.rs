//! Rust type to Protocol Buffers type mapping.
//!
//! The [`ToProtoType`] trait maps Rust types to their protobuf representations.
//! Primitive types map directly; composite types require message definitions.
//!
//! [`ProtoField`] and [`build_message`] provide helpers for constructing
//! protobuf message definitions programmatically.

/// Maps a Rust type to its Protocol Buffers representation.
///
/// Implement this trait for domain types that appear in API endpoints
/// to enable `.proto` file generation via [`ApiToProto`](crate::proto_gen::ApiToProto).
///
/// # Example
///
/// ```ignore
/// struct User { id: u32, name: String }
///
/// impl ToProtoType for User {
///     fn proto_type_name() -> &'static str { "User" }
///     fn is_message() -> bool { true }
///     fn message_definition() -> Option<String> {
///         Some("message User {\n  uint32 id = 1;\n  string name = 2;\n}".to_string())
///     }
/// }
/// ```
pub trait ToProtoType {
    /// The protobuf type name (e.g., `"string"`, `"uint32"`, `"User"`).
    fn proto_type_name() -> &'static str;

    /// Whether this is a message type (composite, needs its own definition).
    fn is_message() -> bool {
        false
    }

    /// Whether this is a repeated field (`Vec<T>`).
    fn is_repeated() -> bool {
        false
    }

    /// Whether this is a map field (`map<K, V>` in proto).
    fn is_map() -> bool {
        false
    }

    /// The protobuf type name of the map key, if this is a map type.
    fn map_key_type() -> Option<&'static str> {
        None
    }

    /// The protobuf type name of the map value, if this is a map type.
    fn map_value_type() -> Option<&'static str> {
        None
    }

    /// Generate the protobuf message definition, if this is a message type.
    ///
    /// Returns `None` for primitive types.
    fn message_definition() -> Option<String> {
        None
    }

    /// Collect all nested message definitions recursively.
    ///
    /// Override this for types that contain nested message types to ensure
    /// all required definitions appear in the generated `.proto` file.
    fn collect_messages() -> Vec<String> {
        Vec::new()
    }

    /// Return the proto fields for this message type.
    ///
    /// Used for request message flattening: when a body type is a message,
    /// its fields can be inlined into the request message instead of being
    /// wrapped in a `body` field.
    fn proto_fields() -> Vec<ProtoField> {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Primitive type mappings
// ---------------------------------------------------------------------------

impl ToProtoType for String {
    fn proto_type_name() -> &'static str {
        "string"
    }
}

impl ToProtoType for &str {
    fn proto_type_name() -> &'static str {
        "string"
    }
}

impl ToProtoType for u32 {
    fn proto_type_name() -> &'static str {
        "uint32"
    }
}

impl ToProtoType for u64 {
    fn proto_type_name() -> &'static str {
        "uint64"
    }
}

impl ToProtoType for i32 {
    fn proto_type_name() -> &'static str {
        "int32"
    }
}

impl ToProtoType for i64 {
    fn proto_type_name() -> &'static str {
        "int64"
    }
}

impl ToProtoType for f32 {
    fn proto_type_name() -> &'static str {
        "float"
    }
}

impl ToProtoType for f64 {
    fn proto_type_name() -> &'static str {
        "double"
    }
}

impl ToProtoType for bool {
    fn proto_type_name() -> &'static str {
        "bool"
    }
}

impl ToProtoType for Vec<u8> {
    fn proto_type_name() -> &'static str {
        "bytes"
    }
}

impl ToProtoType for () {
    fn proto_type_name() -> &'static str {
        "google.protobuf.Empty"
    }
}

impl ToProtoType for http::StatusCode {
    fn proto_type_name() -> &'static str {
        "int32"
    }
}

// ---------------------------------------------------------------------------
// Generic wrapper type mappings
// ---------------------------------------------------------------------------

impl<T: ToProtoType> ToProtoType for Vec<T> {
    fn proto_type_name() -> &'static str {
        T::proto_type_name()
    }

    fn is_repeated() -> bool {
        true
    }

    fn is_message() -> bool {
        T::is_message()
    }

    fn message_definition() -> Option<String> {
        T::message_definition()
    }

    fn collect_messages() -> Vec<String> {
        T::collect_messages()
    }
}

impl<T: ToProtoType> ToProtoType for Option<T> {
    fn proto_type_name() -> &'static str {
        T::proto_type_name()
    }

    fn is_message() -> bool {
        T::is_message()
    }

    fn message_definition() -> Option<String> {
        T::message_definition()
    }

    fn collect_messages() -> Vec<String> {
        T::collect_messages()
    }
}

impl<T: ToProtoType> ToProtoType for Box<T> {
    fn proto_type_name() -> &'static str {
        T::proto_type_name()
    }

    fn is_message() -> bool {
        T::is_message()
    }

    fn message_definition() -> Option<String> {
        T::message_definition()
    }

    fn collect_messages() -> Vec<String> {
        T::collect_messages()
    }
}

impl<T: ToProtoType> ToProtoType for std::sync::Arc<T> {
    fn proto_type_name() -> &'static str {
        T::proto_type_name()
    }

    fn is_message() -> bool {
        T::is_message()
    }

    fn message_definition() -> Option<String> {
        T::message_definition()
    }

    fn collect_messages() -> Vec<String> {
        T::collect_messages()
    }
}

impl<K: ToProtoType, V: ToProtoType> ToProtoType for std::collections::HashMap<K, V> {
    fn proto_type_name() -> &'static str {
        "map"
    }

    fn is_map() -> bool {
        true
    }

    fn map_key_type() -> Option<&'static str> {
        Some(K::proto_type_name())
    }

    fn map_value_type() -> Option<&'static str> {
        Some(V::proto_type_name())
    }

    fn collect_messages() -> Vec<String> {
        let mut msgs = K::collect_messages();
        msgs.extend(V::collect_messages());
        msgs
    }
}

impl<K: ToProtoType, V: ToProtoType> ToProtoType for std::collections::BTreeMap<K, V> {
    fn proto_type_name() -> &'static str {
        "map"
    }

    fn is_map() -> bool {
        true
    }

    fn map_key_type() -> Option<&'static str> {
        Some(K::proto_type_name())
    }

    fn map_value_type() -> Option<&'static str> {
        Some(V::proto_type_name())
    }

    fn collect_messages() -> Vec<String> {
        let mut msgs = K::collect_messages();
        msgs.extend(V::collect_messages());
        msgs
    }
}

// ---------------------------------------------------------------------------
// ProtoField and message building helpers
// ---------------------------------------------------------------------------

/// A single field in a protobuf message definition.
#[derive(Debug, Clone)]
pub struct ProtoField {
    /// Field name (snake_case by protobuf convention).
    pub name: String,
    /// Protobuf type name (e.g., `"string"`, `"uint32"`, `"User"`).
    pub proto_type: String,
    /// Field tag number (must be unique within the message, starting at 1).
    pub tag: u32,
    /// Whether this field is `repeated`.
    pub repeated: bool,
    /// Whether this field is `optional` (proto3 explicit optional).
    pub optional: bool,
    /// Whether this field is a `map<K, V>` type.
    pub is_map: bool,
    /// The protobuf key type for map fields (e.g., `"string"`, `"uint32"`).
    pub map_key_type: Option<String>,
    /// The protobuf value type for map fields (e.g., `"string"`, `"User"`).
    pub map_value_type: Option<String>,
    /// Optional doc comment to emit above the field in the proto definition.
    pub doc: Option<String>,
}

impl ProtoField {
    /// Render this field as one or more lines in a protobuf message definition.
    ///
    /// If a doc comment is present, it is emitted as a proto comment on the
    /// line(s) preceding the field definition.
    pub fn to_proto_line(&self) -> String {
        let field_line = if self.is_map {
            let key = self.map_key_type.as_deref().unwrap_or("string");
            let value = self.map_value_type.as_deref().unwrap_or("string");
            format!("  map<{}, {}> {} = {};", key, value, self.name, self.tag)
        } else {
            let prefix = if self.repeated {
                "repeated "
            } else if self.optional {
                "optional "
            } else {
                ""
            };
            format!("  {}{} {} = {};", prefix, self.proto_type, self.name, self.tag)
        };
        match &self.doc {
            Some(doc) if !doc.is_empty() => {
                let comment_lines: Vec<String> = doc
                    .lines()
                    .map(|line| format!("  // {}", line))
                    .collect();
                let mut result = comment_lines.join("\n");
                result.push('\n');
                result.push_str(&field_line);
                result
            }
            _ => field_line,
        }
    }
}

/// Build a protobuf message definition from a name and a list of fields.
///
/// # Example
///
/// ```
/// use typeway_grpc::mapping::{ProtoField, build_message};
///
/// let msg = build_message("GetUserRequest", &[
///     ProtoField { name: "id".into(), proto_type: "uint32".into(), tag: 1, repeated: false, optional: false, is_map: false, map_key_type: None, map_value_type: None, doc: None },
/// ]);
/// assert!(msg.contains("message GetUserRequest {"));
/// assert!(msg.contains("uint32 id = 1;"));
/// ```
pub fn build_message(name: &str, fields: &[ProtoField]) -> String {
    let mut lines = vec![format!("message {} {{", name)];
    for field in fields {
        lines.push(field.to_proto_line());
    }
    lines.push("}".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_type_names() {
        assert_eq!(String::proto_type_name(), "string");
        assert_eq!(u32::proto_type_name(), "uint32");
        assert_eq!(u64::proto_type_name(), "uint64");
        assert_eq!(i32::proto_type_name(), "int32");
        assert_eq!(i64::proto_type_name(), "int64");
        assert_eq!(f32::proto_type_name(), "float");
        assert_eq!(f64::proto_type_name(), "double");
        assert_eq!(bool::proto_type_name(), "bool");
        assert_eq!(<Vec<u8>>::proto_type_name(), "bytes");
        assert_eq!(<()>::proto_type_name(), "google.protobuf.Empty");
    }

    #[test]
    fn vec_is_repeated() {
        assert!(<Vec<String>>::is_repeated());
        assert!(!String::is_repeated());
    }

    #[test]
    fn option_delegates() {
        assert_eq!(<Option<u32>>::proto_type_name(), "uint32");
        assert!(!<Option<u32>>::is_message());
    }

    #[test]
    fn build_message_output() {
        let msg = build_message(
            "TestMsg",
            &[
                ProtoField {
                    name: "name".into(),
                    proto_type: "string".into(),
                    tag: 1,
                    repeated: false,
                    optional: false,
                    is_map: false,
                    map_key_type: None,
                    map_value_type: None,
                    doc: None,
                },
                ProtoField {
                    name: "ids".into(),
                    proto_type: "uint32".into(),
                    tag: 2,
                    repeated: true,
                    optional: false,
                    is_map: false,
                    map_key_type: None,
                    map_value_type: None,
                    doc: None,
                },
            ],
        );
        assert!(msg.contains("message TestMsg {"));
        assert!(msg.contains("  string name = 1;"));
        assert!(msg.contains("  repeated uint32 ids = 2;"));
        assert!(msg.ends_with('}'));
    }

    #[test]
    fn proto_field_optional() {
        let field = ProtoField {
            name: "email".into(),
            proto_type: "string".into(),
            tag: 1,
            repeated: false,
            optional: true,
            is_map: false,
            map_key_type: None,
            map_value_type: None,
            doc: None,
        };
        assert_eq!(field.to_proto_line(), "  optional string email = 1;");
    }

    #[test]
    fn hashmap_is_map() {
        assert!(<std::collections::HashMap<String, u32>>::is_map());
        assert_eq!(
            <std::collections::HashMap<String, u32>>::map_key_type(),
            Some("string")
        );
        assert_eq!(
            <std::collections::HashMap<String, u32>>::map_value_type(),
            Some("uint32")
        );
        assert_eq!(
            <std::collections::HashMap<String, u32>>::proto_type_name(),
            "map"
        );
    }

    #[test]
    fn btreemap_is_map() {
        assert!(<std::collections::BTreeMap<String, u32>>::is_map());
        assert_eq!(
            <std::collections::BTreeMap<String, u32>>::map_key_type(),
            Some("string")
        );
        assert_eq!(
            <std::collections::BTreeMap<String, u32>>::map_value_type(),
            Some("uint32")
        );
    }

    #[test]
    fn proto_field_map() {
        let field = ProtoField {
            name: "metadata".into(),
            proto_type: "map".into(),
            tag: 1,
            repeated: false,
            optional: false,
            is_map: true,
            map_key_type: Some("string".into()),
            map_value_type: Some("uint32".into()),
            doc: None,
        };
        assert_eq!(field.to_proto_line(), "  map<string, uint32> metadata = 1;");
    }

    #[test]
    fn proto_field_map_with_doc() {
        let field = ProtoField {
            name: "labels".into(),
            proto_type: "map".into(),
            tag: 2,
            repeated: false,
            optional: false,
            is_map: true,
            map_key_type: Some("string".into()),
            map_value_type: Some("string".into()),
            doc: Some("Key-value labels".into()),
        };
        let line = field.to_proto_line();
        assert!(line.contains("// Key-value labels"));
        assert!(line.contains("map<string, string> labels = 2;"));
    }

    #[test]
    fn build_message_with_map_field() {
        let msg = build_message(
            "Config",
            &[
                ProtoField {
                    name: "name".into(),
                    proto_type: "string".into(),
                    tag: 1,
                    repeated: false,
                    optional: false,
                    is_map: false,
                    map_key_type: None,
                    map_value_type: None,
                    doc: None,
                },
                ProtoField {
                    name: "metadata".into(),
                    proto_type: "map".into(),
                    tag: 2,
                    repeated: false,
                    optional: false,
                    is_map: true,
                    map_key_type: Some("string".into()),
                    map_value_type: Some("string".into()),
                    doc: None,
                },
            ],
        );
        assert!(msg.contains("message Config {"));
        assert!(msg.contains("  string name = 1;"));
        assert!(msg.contains("  map<string, string> metadata = 2;"));
    }

    #[test]
    fn status_code_maps_to_int32() {
        assert_eq!(http::StatusCode::proto_type_name(), "int32");
    }
}
