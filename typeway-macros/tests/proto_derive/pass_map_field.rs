use std::collections::HashMap;
use typeway_grpc::ToProtoType;
use typeway_macros::ToProtoType;

#[derive(ToProtoType)]
struct Config {
    #[proto(tag = 1)]
    name: String,
    #[proto(tag = 2)]
    metadata: HashMap<String, String>,
    #[proto(tag = 3)]
    counts: HashMap<String, u32>,
}

fn main() {
    assert_eq!(Config::proto_type_name(), "Config");
    assert!(Config::is_message());

    let def = Config::message_definition().unwrap();
    assert!(def.contains("string name = 1"), "def: {}", def);
    assert!(
        def.contains("map<string, string> metadata = 2"),
        "Expected map<string, string> metadata = 2 in: {}",
        def
    );
    assert!(
        def.contains("map<string, uint32> counts = 3"),
        "Expected map<string, uint32> counts = 3 in: {}",
        def
    );

    // Verify proto_fields returns the map fields correctly.
    let fields = Config::proto_fields();
    assert_eq!(fields.len(), 3);
    assert!(!fields[0].is_map);
    assert!(fields[1].is_map);
    assert_eq!(fields[1].map_key_type.as_deref(), Some("string"));
    assert_eq!(fields[1].map_value_type.as_deref(), Some("string"));
    assert!(fields[2].is_map);
    assert_eq!(fields[2].map_key_type.as_deref(), Some("string"));
    assert_eq!(fields[2].map_value_type.as_deref(), Some("uint32"));
}
