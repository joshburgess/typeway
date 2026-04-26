use typeway_grpc::diff::{diff_protos, ChangeKind};

const USERS_V1: &str = include_str!("fixtures/users.proto");
const USERS_V2: &str = include_str!("fixtures/users_v2.proto");

#[test]
fn no_changes_detected() {
    let changes = diff_protos(USERS_V1, USERS_V1).unwrap();
    assert!(
        changes.is_empty(),
        "Expected no changes, got: {:?}",
        changes
    );
}

#[test]
fn added_rpc_is_compatible() {
    let old = r#"syntax = "proto3";
package test.v1;
service Svc {
  rpc GetUser(Req) returns (Resp);
}
message Req {
  string id = 1;
}
message Resp {
  string name = 1;
}
"#;
    let new = r#"syntax = "proto3";
package test.v1;
service Svc {
  rpc GetUser(Req) returns (Resp);
  rpc CreateUser(CreateReq) returns (Resp);
}
message Req {
  string id = 1;
}
message Resp {
  string name = 1;
}
message CreateReq {
  string name = 1;
}
"#;
    let changes = diff_protos(old, new).unwrap();
    let added: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Compatible && c.description == "RPC method added")
        .collect();
    assert_eq!(added.len(), 1);
    assert!(added[0].location.contains("CreateUser"));
}

#[test]
fn removed_rpc_is_breaking() {
    let old = r#"syntax = "proto3";
package test.v1;
service Svc {
  rpc GetUser(Req) returns (Resp);
  rpc DeleteUser(Req) returns (Resp);
}
message Req {
  string id = 1;
}
message Resp {
  string name = 1;
}
"#;
    let new = r#"syntax = "proto3";
package test.v1;
service Svc {
  rpc GetUser(Req) returns (Resp);
}
message Req {
  string id = 1;
}
message Resp {
  string name = 1;
}
"#;
    let changes = diff_protos(old, new).unwrap();
    let removed: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Breaking && c.description == "RPC method removed")
        .collect();
    assert_eq!(removed.len(), 1);
    assert!(removed[0].location.contains("DeleteUser"));
}

#[test]
fn changed_input_type_is_breaking() {
    let old = r#"syntax = "proto3";
package test.v1;
service Svc {
  rpc GetUser(Req) returns (Resp);
}
message Req {
  string id = 1;
}
message Req2 {
  uint32 id = 1;
}
message Resp {
  string name = 1;
}
"#;
    let new = r#"syntax = "proto3";
package test.v1;
service Svc {
  rpc GetUser(Req2) returns (Resp);
}
message Req {
  string id = 1;
}
message Req2 {
  uint32 id = 1;
}
message Resp {
  string name = 1;
}
"#;
    let changes = diff_protos(old, new).unwrap();
    let breaking: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Breaking && c.description.contains("input type changed"))
        .collect();
    assert_eq!(breaking.len(), 1);
}

#[test]
fn changed_output_type_is_breaking() {
    let old = r#"syntax = "proto3";
package test.v1;
service Svc {
  rpc GetUser(Req) returns (Resp);
}
message Req {
  string id = 1;
}
message Resp {
  string name = 1;
}
message Resp2 {
  uint32 id = 1;
}
"#;
    let new = r#"syntax = "proto3";
package test.v1;
service Svc {
  rpc GetUser(Req) returns (Resp2);
}
message Req {
  string id = 1;
}
message Resp {
  string name = 1;
}
message Resp2 {
  uint32 id = 1;
}
"#;
    let changes = diff_protos(old, new).unwrap();
    let breaking: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Breaking && c.description.contains("output type changed"))
        .collect();
    assert_eq!(breaking.len(), 1);
}

#[test]
fn added_field_is_compatible() {
    let old = r#"syntax = "proto3";
package test.v1;
message User {
  uint32 id = 1;
  string name = 2;
}
"#;
    let new = r#"syntax = "proto3";
package test.v1;
message User {
  uint32 id = 1;
  string name = 2;
  string email = 3;
}
"#;
    let changes = diff_protos(old, new).unwrap();
    let compatible: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Compatible && c.description.contains("field added"))
        .collect();
    assert_eq!(compatible.len(), 1);
    assert!(compatible[0].location.contains("email"));
}

#[test]
fn removed_field_is_breaking() {
    let old = r#"syntax = "proto3";
package test.v1;
message User {
  uint32 id = 1;
  string name = 2;
  string email = 3;
}
"#;
    let new = r#"syntax = "proto3";
package test.v1;
message User {
  uint32 id = 1;
  string name = 2;
}
"#;
    let changes = diff_protos(old, new).unwrap();
    let breaking: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Breaking && c.description.contains("field removed"))
        .collect();
    assert_eq!(breaking.len(), 1);
    assert!(breaking[0].location.contains("email"));
}

#[test]
fn changed_field_type_is_breaking() {
    let old = r#"syntax = "proto3";
package test.v1;
message User {
  uint32 id = 1;
  string name = 2;
}
"#;
    let new = r#"syntax = "proto3";
package test.v1;
message User {
  string id = 1;
  string name = 2;
}
"#;
    let changes = diff_protos(old, new).unwrap();
    let breaking: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Breaking && c.description.contains("type changed"))
        .collect();
    assert_eq!(breaking.len(), 1);
}

#[test]
fn renamed_field_is_compatible() {
    let old = r#"syntax = "proto3";
package test.v1;
message User {
  uint32 id = 1;
  string name = 2;
}
"#;
    let new = r#"syntax = "proto3";
package test.v1;
message User {
  uint32 user_id = 1;
  string name = 2;
}
"#;
    let changes = diff_protos(old, new).unwrap();
    let compatible: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Compatible && c.description.contains("field renamed"))
        .collect();
    assert_eq!(compatible.len(), 1);
}

#[test]
fn added_message_is_compatible() {
    let old = r#"syntax = "proto3";
package test.v1;
message User {
  uint32 id = 1;
}
"#;
    let new = r#"syntax = "proto3";
package test.v1;
message User {
  uint32 id = 1;
}
message Address {
  string street = 1;
}
"#;
    let changes = diff_protos(old, new).unwrap();
    let compatible: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Compatible && c.description == "message added")
        .collect();
    assert_eq!(compatible.len(), 1);
    assert_eq!(compatible[0].location, "Address");
}

#[test]
fn removed_message_is_breaking() {
    let old = r#"syntax = "proto3";
package test.v1;
message User {
  uint32 id = 1;
}
message Address {
  string street = 1;
}
"#;
    let new = r#"syntax = "proto3";
package test.v1;
message User {
  uint32 id = 1;
}
"#;
    let changes = diff_protos(old, new).unwrap();
    let breaking: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Breaking && c.description == "message removed")
        .collect();
    assert_eq!(breaking.len(), 1);
    assert_eq!(breaking[0].location, "Address");
}

#[test]
fn mixed_changes() {
    // v1 -> v2: DeleteUser removed (breaking), UpdateUser added (compatible),
    // User.name renamed to full_name (compatible), User.phone added (compatible).
    let changes = diff_protos(USERS_V1, USERS_V2).unwrap();

    let breaking: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Breaking)
        .collect();
    let compatible: Vec<_> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Compatible)
        .collect();

    // Breaking: DeleteUser RPC removed
    assert!(
        breaking
            .iter()
            .any(|c| c.description == "RPC method removed" && c.location.contains("DeleteUser")),
        "Expected DeleteUser removal as breaking: {:?}",
        breaking
    );

    // Compatible: UpdateUser RPC added
    assert!(
        compatible
            .iter()
            .any(|c| c.description == "RPC method added" && c.location.contains("UpdateUser")),
        "Expected UpdateUser addition as compatible: {:?}",
        compatible
    );

    // Compatible: User.name -> User.full_name (renamed, tag 2)
    assert!(
        compatible
            .iter()
            .any(|c| c.description.contains("field renamed")),
        "Expected field rename as compatible: {:?}",
        compatible
    );

    // Compatible: User.phone added (tag 4)
    assert!(
        compatible
            .iter()
            .any(|c| c.description.contains("field added") && c.location.contains("phone")),
        "Expected phone field addition as compatible: {:?}",
        compatible
    );

    // There should also be a compatible change for UpdateUserRequest message added.
    assert!(
        compatible
            .iter()
            .any(|c| c.description == "message added" && c.location.contains("UpdateUserRequest")),
        "Expected UpdateUserRequest message addition: {:?}",
        compatible
    );
}
