//! Proto file diff tool for detecting breaking changes.
//!
//! Compares two parsed `.proto` files and reports breaking vs compatible
//! changes. Useful for CI pipelines that need to guard against accidental
//! proto contract breakage.
//!
//! # Breaking changes
//!
//! - Service or RPC method removed
//! - RPC input or output type changed
//! - Message removed
//! - Message field removed (by tag)
//! - Message field type changed
//!
//! # Compatible changes
//!
//! - Service, RPC method, or message added
//! - Message field added (new tags are backward compatible in proto3)
//! - Field renamed (proto3 uses tags on the wire, not names)
//! - Comment changes

use crate::proto_parse::{self, ProtoFile};

/// A change detected between two proto file versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtoChange {
    /// The kind of change (breaking or compatible).
    pub kind: ChangeKind,
    /// Location of the change (e.g., `"UserService.GetUser"`, `"User.email"`).
    pub location: String,
    /// Human-readable description of the change.
    pub description: String,
}

/// Whether a change is breaking or backward-compatible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Safe change -- existing clients will not break.
    Compatible,
    /// Breaking change -- may break existing clients.
    Breaking,
}

/// Compare two `.proto` file strings and return detected changes.
///
/// Parses both files and walks services and messages, reporting all
/// breaking and compatible changes found.
///
/// # Errors
///
/// Returns a description if either file fails to parse.
pub fn diff_protos(old: &str, new: &str) -> Result<Vec<ProtoChange>, String> {
    let old_file = proto_parse::parse_proto(old)?;
    let new_file = proto_parse::parse_proto(new)?;

    let mut changes = Vec::new();
    diff_services(&old_file, &new_file, &mut changes);
    diff_messages(&old_file, &new_file, &mut changes);
    Ok(changes)
}

fn diff_services(old: &ProtoFile, new: &ProtoFile, changes: &mut Vec<ProtoChange>) {
    for old_svc in &old.services {
        let new_svc = new.services.iter().find(|s| s.name == old_svc.name);
        let new_svc = match new_svc {
            Some(s) => s,
            None => {
                changes.push(ProtoChange {
                    kind: ChangeKind::Breaking,
                    location: old_svc.name.clone(),
                    description: "service removed".to_string(),
                });
                continue;
            }
        };

        // Check each method in the old service.
        for old_method in &old_svc.methods {
            let new_method = new_svc.methods.iter().find(|m| m.name == old_method.name);
            match new_method {
                None => {
                    changes.push(ProtoChange {
                        kind: ChangeKind::Breaking,
                        location: format!("{}.{}", old_svc.name, old_method.name),
                        description: "RPC method removed".to_string(),
                    });
                }
                Some(nm) => {
                    if nm.input_type != old_method.input_type {
                        changes.push(ProtoChange {
                            kind: ChangeKind::Breaking,
                            location: format!("{}.{}", old_svc.name, old_method.name),
                            description: format!(
                                "input type changed: {} -> {}",
                                old_method.input_type, nm.input_type
                            ),
                        });
                    }
                    if nm.output_type != old_method.output_type {
                        changes.push(ProtoChange {
                            kind: ChangeKind::Breaking,
                            location: format!("{}.{}", old_svc.name, old_method.name),
                            description: format!(
                                "output type changed: {} -> {}",
                                old_method.output_type, nm.output_type
                            ),
                        });
                    }
                }
            }
        }

        // Check for added methods.
        for new_method in &new_svc.methods {
            if !old_svc.methods.iter().any(|m| m.name == new_method.name) {
                changes.push(ProtoChange {
                    kind: ChangeKind::Compatible,
                    location: format!("{}.{}", new_svc.name, new_method.name),
                    description: "RPC method added".to_string(),
                });
            }
        }
    }

    // Check for added services.
    for new_svc in &new.services {
        if !old.services.iter().any(|s| s.name == new_svc.name) {
            changes.push(ProtoChange {
                kind: ChangeKind::Compatible,
                location: new_svc.name.clone(),
                description: "service added".to_string(),
            });
        }
    }
}

fn diff_messages(old: &ProtoFile, new: &ProtoFile, changes: &mut Vec<ProtoChange>) {
    for old_msg in &old.messages {
        let new_msg = new.messages.iter().find(|m| m.name == old_msg.name);
        let new_msg = match new_msg {
            Some(m) => m,
            None => {
                changes.push(ProtoChange {
                    kind: ChangeKind::Breaking,
                    location: old_msg.name.clone(),
                    description: "message removed".to_string(),
                });
                continue;
            }
        };

        // Check each field by tag (tag is the identity, not name).
        for old_field in &old_msg.fields {
            let new_field = new_msg.fields.iter().find(|f| f.tag == old_field.tag);
            match new_field {
                None => {
                    changes.push(ProtoChange {
                        kind: ChangeKind::Breaking,
                        location: format!("{}.{}", old_msg.name, old_field.name),
                        description: format!("field removed (tag {})", old_field.tag),
                    });
                }
                Some(nf) => {
                    if nf.proto_type != old_field.proto_type {
                        changes.push(ProtoChange {
                            kind: ChangeKind::Breaking,
                            location: format!("{}.{}", old_msg.name, old_field.name),
                            description: format!(
                                "type changed: {} -> {}",
                                old_field.proto_type, nf.proto_type
                            ),
                        });
                    }
                    if nf.name != old_field.name {
                        changes.push(ProtoChange {
                            kind: ChangeKind::Compatible,
                            location: format!("{} (tag {})", old_msg.name, old_field.tag),
                            description: format!(
                                "field renamed: {} -> {}",
                                old_field.name, nf.name
                            ),
                        });
                    }
                }
            }
        }

        // Check for added fields.
        for new_field in &new_msg.fields {
            if !old_msg.fields.iter().any(|f| f.tag == new_field.tag) {
                changes.push(ProtoChange {
                    kind: ChangeKind::Compatible,
                    location: format!("{}.{}", new_msg.name, new_field.name),
                    description: format!("field added (tag {})", new_field.tag),
                });
            }
        }
    }

    // Check for added messages.
    for new_msg in &new.messages {
        if !old.messages.iter().any(|m| m.name == new_msg.name) {
            changes.push(ProtoChange {
                kind: ChangeKind::Compatible,
                location: new_msg.name.clone(),
                description: "message added".to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = r#"syntax = "proto3";
package users.v1;
service UserService {
  rpc GetUser(GetUserRequest) returns (User);
  rpc ListUser(google.protobuf.Empty) returns (ListUserResponse);
}
message User {
  uint32 id = 1;
  string name = 2;
}
message GetUserRequest {
  string param1 = 1;
}
message ListUserResponse {
  repeated User users = 1;
}
"#;

    #[test]
    fn no_changes_on_identical() {
        let changes = diff_protos(V1, V1).unwrap();
        assert!(changes.is_empty());
    }

    #[test]
    fn added_rpc_is_compatible() {
        let v2 = r#"syntax = "proto3";
package users.v1;
service UserService {
  rpc GetUser(GetUserRequest) returns (User);
  rpc ListUser(google.protobuf.Empty) returns (ListUserResponse);
  rpc CreateUser(CreateUserRequest) returns (User);
}
message User {
  uint32 id = 1;
  string name = 2;
}
message GetUserRequest {
  string param1 = 1;
}
message ListUserResponse {
  repeated User users = 1;
}
message CreateUserRequest {
  string name = 1;
}
"#;
        let changes = diff_protos(V1, v2).unwrap();
        let added: Vec<_> = changes
            .iter()
            .filter(|c| {
                c.kind == ChangeKind::Compatible && c.description.contains("RPC method added")
            })
            .collect();
        assert_eq!(added.len(), 1);
        assert!(added[0].location.contains("CreateUser"));
    }

    #[test]
    fn removed_rpc_is_breaking() {
        let v2 = r#"syntax = "proto3";
package users.v1;
service UserService {
  rpc GetUser(GetUserRequest) returns (User);
}
message User {
  uint32 id = 1;
  string name = 2;
}
message GetUserRequest {
  string param1 = 1;
}
"#;
        let changes = diff_protos(V1, v2).unwrap();
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| {
                c.kind == ChangeKind::Breaking && c.description.contains("RPC method removed")
            })
            .collect();
        assert_eq!(breaking.len(), 1);
        assert!(breaking[0].location.contains("ListUser"));
    }

    #[test]
    fn changed_input_type_is_breaking() {
        let v2 = V1.replace(
            "rpc GetUser(GetUserRequest) returns (User);",
            "rpc GetUser(DifferentRequest) returns (User);",
        );
        // Add the new message so it parses.
        let v2 = format!(
            "{}\nmessage DifferentRequest {{\n  string id = 1;\n}}\n",
            v2
        );
        let changes = diff_protos(V1, &v2).unwrap();
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| {
                c.kind == ChangeKind::Breaking && c.description.contains("input type changed")
            })
            .collect();
        assert_eq!(breaking.len(), 1);
    }

    #[test]
    fn changed_output_type_is_breaking() {
        let v2 = V1.replace(
            "rpc GetUser(GetUserRequest) returns (User);",
            "rpc GetUser(GetUserRequest) returns (UserV2);",
        );
        let v2 = format!("{}\nmessage UserV2 {{\n  uint32 id = 1;\n}}\n", v2);
        let changes = diff_protos(V1, &v2).unwrap();
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| {
                c.kind == ChangeKind::Breaking && c.description.contains("output type changed")
            })
            .collect();
        assert_eq!(breaking.len(), 1);
    }

    #[test]
    fn added_field_is_compatible() {
        let v2 = V1.replace(
            "message User {\n  uint32 id = 1;\n  string name = 2;\n}",
            "message User {\n  uint32 id = 1;\n  string name = 2;\n  string email = 3;\n}",
        );
        let changes = diff_protos(V1, &v2).unwrap();
        let compatible: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::Compatible && c.description.contains("field added"))
            .collect();
        assert_eq!(compatible.len(), 1);
        assert!(compatible[0].location.contains("email"));
    }

    #[test]
    fn removed_field_is_breaking() {
        let v2 = V1.replace(
            "message User {\n  uint32 id = 1;\n  string name = 2;\n}",
            "message User {\n  uint32 id = 1;\n}",
        );
        let changes = diff_protos(V1, &v2).unwrap();
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::Breaking && c.description.contains("field removed"))
            .collect();
        assert_eq!(breaking.len(), 1);
        assert!(breaking[0].location.contains("name"));
    }

    #[test]
    fn changed_field_type_is_breaking() {
        let v2 = V1.replace(
            "message User {\n  uint32 id = 1;\n  string name = 2;\n}",
            "message User {\n  string id = 1;\n  string name = 2;\n}",
        );
        let changes = diff_protos(V1, &v2).unwrap();
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::Breaking && c.description.contains("type changed"))
            .collect();
        assert_eq!(breaking.len(), 1);
    }

    #[test]
    fn renamed_field_is_compatible() {
        let v2 = V1.replace(
            "message User {\n  uint32 id = 1;\n  string name = 2;\n}",
            "message User {\n  uint32 user_id = 1;\n  string name = 2;\n}",
        );
        let changes = diff_protos(V1, &v2).unwrap();
        let compatible: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::Compatible && c.description.contains("field renamed"))
            .collect();
        assert_eq!(compatible.len(), 1);
    }

    #[test]
    fn added_message_is_compatible() {
        let v2 = format!("{}\nmessage NewMessage {{\n  string value = 1;\n}}\n", V1);
        let changes = diff_protos(V1, &v2).unwrap();
        let compatible: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::Compatible && c.description.contains("message added"))
            .collect();
        assert_eq!(compatible.len(), 1);
        assert!(compatible[0].location.contains("NewMessage"));
    }

    #[test]
    fn removed_message_is_breaking() {
        // Remove the ListUserResponse message.
        let v2 = V1.replace(
            "message ListUserResponse {\n  repeated User users = 1;\n}\n",
            "",
        );
        let changes = diff_protos(V1, &v2).unwrap();
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::Breaking && c.description.contains("message removed"))
            .collect();
        assert_eq!(breaking.len(), 1);
        assert!(breaking[0].location.contains("ListUserResponse"));
    }
}
