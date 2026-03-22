//! Round-trip test: generate .proto from Rust types, then regenerate Rust code.

use typeway_grpc::codegen::{proto_to_typeway, proto_to_typeway_with_codec};

const SAMPLE_PROTO: &str = r#"syntax = "proto3";

package trading.v1;

service OrderBook {
  // POST /orders
  rpc SubmitOrder(Order) returns (OrderAck);
  // POST /cancel
  rpc CancelOrder(CancelRequest) returns (CancelAck);
  // POST /book
  rpc GetOrderBook(SymbolQuery) returns (OrderBookSnapshot);
}

message Order {
  string symbol = 1;
  string side = 2;
  double price = 3;
  uint32 quantity = 4;
}

message OrderAck {
  string order_id = 1;
  string status = 2;
}

message CancelRequest {
  string order_id = 1;
}

message CancelAck {
  string order_id = 1;
  string status = 2;
}

message SymbolQuery {
  string symbol = 1;
}

message PriceLevel {
  double price = 1;
  uint32 quantity = 2;
}

message OrderBookSnapshot {
  string symbol = 1;
  repeated PriceLevel bids = 2;
  repeated PriceLevel asks = 3;
}
"#;

#[test]
fn serde_codegen_produces_valid_rust() {
    let output = proto_to_typeway(SAMPLE_PROTO).unwrap();

    // Structs present.
    assert!(output.contains("pub struct Order {"));
    assert!(output.contains("pub struct OrderAck {"));
    assert!(output.contains("pub struct CancelRequest {"));
    assert!(output.contains("pub struct PriceLevel {"));
    assert!(output.contains("pub struct OrderBookSnapshot {"));

    // Fields correct (serde mode uses String).
    assert!(output.contains("pub symbol: String,"));
    assert!(output.contains("pub price: f64,"));
    assert!(output.contains("pub quantity: u32,"));
    assert!(!output.contains("BytesStr"), "serde mode should not use BytesStr");

    // Repeated fields.
    assert!(output.contains("pub bids: Vec<PriceLevel>,"));
    assert!(output.contains("pub asks: Vec<PriceLevel>,"));

    // Serde derives (no TypewayCodec).
    assert!(output.contains("Serialize, Deserialize"));
    assert!(!output.contains("TypewayCodec"));
    assert!(!output.contains("#[proto(tag"));

    // API type.
    assert!(output.contains("type API = ("));
    assert!(output.contains("PostEndpoint"));

    // Path declarations.
    assert!(output.contains("typeway_path!"));
}

#[test]
fn codec_codegen_produces_valid_rust() {
    let output = proto_to_typeway_with_codec(SAMPLE_PROTO).unwrap();

    // TypewayCodec derives.
    assert!(output.contains("TypewayCodec"));
    assert!(output.contains("Default"));

    // Proto tag attributes.
    assert!(output.contains("#[proto(tag = 1)]"));
    assert!(output.contains("#[proto(tag = 2)]"));
    assert!(output.contains("#[proto(tag = 3)]"));
    assert!(output.contains("#[proto(tag = 4)]"));

    // Imports.
    assert!(output.contains("use typeway_macros::TypewayCodec;"));
    assert!(output.contains("use typeway_protobuf::BytesStr;"));

    // All structs present.
    assert!(output.contains("pub struct Order {"));
    assert!(output.contains("pub struct OrderBookSnapshot {"));
    assert!(output.contains("pub struct PriceLevel {"));

    // String fields use BytesStr in codec mode.
    assert!(output.contains("pub symbol: BytesStr,"), "Expected BytesStr for string fields");
    assert!(output.contains("pub order_id: BytesStr,"));

    // Non-string fields unchanged.
    assert!(output.contains("pub price: f64,"));
    assert!(output.contains("pub quantity: u32,"));

    // Repeated fields preserved.
    assert!(output.contains("pub bids: Vec<PriceLevel>,"));

    // API type still generated.
    assert!(output.contains("type API = ("));
}

#[test]
fn all_proto_types_map_correctly() {
    let proto = r#"syntax = "proto3";
package test.v1;

message AllTypes {
  string str_field = 1;
  uint32 u32_field = 2;
  uint64 u64_field = 3;
  int32 i32_field = 4;
  int64 i64_field = 5;
  float f32_field = 6;
  double f64_field = 7;
  bool bool_field = 8;
  bytes bytes_field = 9;
  sint32 signed32 = 10;
  sint64 signed64 = 11;
  fixed32 fixed32_field = 12;
  fixed64 fixed64_field = 13;
  sfixed32 sfixed32_field = 14;
  sfixed64 sfixed64_field = 15;
}
"#;
    let output = proto_to_typeway_with_codec(proto).unwrap();
    // Codec mode: string → BytesStr.
    assert!(output.contains("pub str_field: BytesStr,"));
    assert!(output.contains("pub u32_field: u32,"));
    assert!(output.contains("pub u64_field: u64,"));
    assert!(output.contains("pub i32_field: i32,"));
    assert!(output.contains("pub i64_field: i64,"));
    assert!(output.contains("pub f32_field: f32,"));
    assert!(output.contains("pub f64_field: f64,"));
    assert!(output.contains("pub bool_field: bool,"));
    assert!(output.contains("pub bytes_field: Vec<u8>,"));
}

#[test]
fn optional_fields_generate_option() {
    let proto = r#"syntax = "proto3";
package test.v1;

message WithOptional {
  string name = 1;
  optional string nickname = 2;
  optional uint32 age = 3;
}
"#;
    let output = proto_to_typeway_with_codec(proto).unwrap();
    // Codec mode: string → BytesStr, optional string → Option<BytesStr>.
    assert!(output.contains("pub name: BytesStr,"));
    assert!(output.contains("pub nickname: Option<BytesStr>,"));
    assert!(output.contains("pub age: Option<u32>,"));
}

#[test]
fn repeated_fields_generate_vec() {
    let proto = r#"syntax = "proto3";
package test.v1;

message WithRepeated {
  repeated string tags = 1;
  repeated uint32 scores = 2;
  repeated Nested items = 3;
}

message Nested {
  string value = 1;
}
"#;
    let output = proto_to_typeway_with_codec(proto).unwrap();
    // Codec mode: repeated string → Vec<BytesStr>.
    assert!(output.contains("pub tags: Vec<BytesStr>,"));
    assert!(output.contains("pub scores: Vec<u32>,"));
    assert!(output.contains("pub items: Vec<Nested>,"));
}

#[test]
fn map_fields_generate_hashmap() {
    let proto = r#"syntax = "proto3";
package test.v1;

message Config {
  map<string, string> metadata = 1;
  map<string, uint32> counters = 2;
}
"#;
    let output = proto_to_typeway_with_codec(proto).unwrap();
    // Codec mode: map keys and values that are strings use BytesStr.
    assert!(output.contains("std::collections::HashMap<BytesStr, BytesStr>"));
    assert!(output.contains("std::collections::HashMap<BytesStr, u32>"));
}

#[test]
fn nested_messages_flattened() {
    let proto = r#"syntax = "proto3";
package test.v1;

message Outer {
  string name = 1;
  message Inner {
    uint32 id = 1;
  }
  Inner detail = 2;
}
"#;
    let output = proto_to_typeway_with_codec(proto).unwrap();
    // Nested message flattened with underscore.
    assert!(output.contains("pub struct Outer_Inner {"));
    assert!(output.contains("pub struct Outer {"));
}

#[test]
fn empty_message_generates_empty_struct() {
    let proto = r#"syntax = "proto3";
package test.v1;

service Ping {
  // GET /ping
  rpc Ping(google.protobuf.Empty) returns (google.protobuf.Empty);
}
"#;
    let output = proto_to_typeway_with_codec(proto).unwrap();
    // google.protobuf.Empty maps to () in the endpoint type.
    assert!(output.contains("type API = ("));
}
