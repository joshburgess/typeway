#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use typeway_macros::TypewaySchema;
use typeway_openapi::ToSchema;

/// A user account.
#[derive(TypewaySchema)]
struct User {
    /// The unique user identifier.
    id: u32,
    /// The user's display name.
    name: String,
    /// Email address (must be unique).
    email: String,
    /// Short biography.
    bio: Option<String>,
}

#[test]
fn schema_has_correct_type_and_properties() {
    let schema = User::schema();
    assert_eq!(schema.schema_type.as_deref(), Some("object"));
    let props = schema.properties.as_ref().unwrap();
    assert_eq!(props.len(), 4);
    assert!(props.contains_key("id"));
    assert!(props.contains_key("name"));
    assert!(props.contains_key("email"));
    assert!(props.contains_key("bio"));
}

#[test]
fn field_descriptions_are_propagated() {
    let schema = User::schema();
    let props = schema.properties.as_ref().unwrap();
    assert_eq!(
        props["id"].description.as_deref(),
        Some("The unique user identifier.")
    );
    assert_eq!(
        props["name"].description.as_deref(),
        Some("The user's display name.")
    );
    assert_eq!(
        props["email"].description.as_deref(),
        Some("Email address (must be unique).")
    );
    assert_eq!(
        props["bio"].description.as_deref(),
        Some("Short biography.")
    );
}

#[test]
fn struct_description_is_propagated() {
    let schema = User::schema();
    assert_eq!(schema.description.as_deref(), Some("A user account."));
}

#[test]
fn type_name_matches_struct_name() {
    assert_eq!(User::type_name(), "User");
}

#[derive(Serialize, Deserialize, TypewaySchema)]
#[serde(rename_all = "camelCase")]
struct CamelArticle {
    /// Article title.
    article_title: String,
    /// Publication date.
    pub_date: String,
    /// Explicit rename overrides rename_all.
    #[serde(rename = "TAG_LIST")]
    tag_list: Vec<String>,
}

#[test]
fn camel_case_rename_works() {
    let schema = CamelArticle::schema();
    let props = schema.properties.as_ref().unwrap();
    assert!(props.contains_key("articleTitle"));
    assert!(props.contains_key("pubDate"));
    // Explicit rename takes priority.
    assert!(props.contains_key("TAG_LIST"));
    assert!(!props.contains_key("tagList"));
}

#[derive(TypewaySchema)]
struct NoDocStruct {
    x: u32,
    y: String,
}

#[test]
fn fields_without_docs_have_no_description() {
    let schema = NoDocStruct::schema();
    assert!(schema.description.is_none());
    let props = schema.properties.as_ref().unwrap();
    assert!(props["x"].description.is_none());
    assert!(props["y"].description.is_none());
}

// ---------------------------------------------------------------------------
// Enum schema tests
// ---------------------------------------------------------------------------

/// The status of an order.
#[derive(TypewaySchema)]
enum Status {
    Pending,
    Active,
    Closed,
}

#[test]
fn unit_only_enum_emits_string_enum() {
    let schema = Status::schema();
    assert_eq!(schema.schema_type.as_deref(), Some("string"));
    assert!(schema.one_of.is_none());

    let values = schema.enum_values.as_ref().expect("enum values present");
    let names: Vec<&str> = values.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(names, vec!["Pending", "Active", "Closed"]);
    assert_eq!(
        schema.description.as_deref(),
        Some("The status of an order.")
    );
}

#[derive(Serialize, Deserialize, TypewaySchema)]
#[serde(rename_all = "kebab-case")]
enum LogLevel {
    Trace,
    DebugInfo,
    Warning,
    #[serde(rename = "FATAL")]
    Fatal,
}

#[test]
fn enum_rename_all_and_per_variant_rename() {
    let schema = LogLevel::schema();
    let values = schema.enum_values.as_ref().expect("enum values present");
    let names: Vec<&str> = values.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(names, vec!["trace", "debug-info", "warning", "FATAL"]);
}

#[derive(Serialize, Deserialize, TypewaySchema)]
enum Event {
    /// A log line was emitted.
    Log(String),
    Counter {
        name: String,
        value: u64,
    },
    Heartbeat,
}

#[test]
fn externally_tagged_enum_emits_one_of() {
    let schema = Event::schema();
    assert!(schema.schema_type.is_none());
    assert!(schema.discriminator.is_none());

    let variants = schema.one_of.as_ref().expect("oneOf variants present");
    assert_eq!(variants.len(), 3);

    // Log(String) → {"type":"object","properties":{"Log": <string>}}
    let log = &variants[0];
    let log_props = log.properties.as_ref().unwrap();
    assert_eq!(log_props["Log"].schema_type.as_deref(), Some("string"));
    assert_eq!(log.description.as_deref(), Some("A log line was emitted."));

    // Counter { name, value } → {"properties":{"Counter": {object with name+value}}}
    let counter = &variants[1];
    let inner = counter.properties.as_ref().unwrap()["Counter"]
        .properties
        .as_ref()
        .unwrap();
    assert!(inner.contains_key("name"));
    assert!(inner.contains_key("value"));

    // Heartbeat (unit) → {"type":"string","enum":["Heartbeat"]}
    let hb = &variants[2];
    assert_eq!(hb.schema_type.as_deref(), Some("string"));
    let hb_values: Vec<&str> = hb
        .enum_values
        .as_ref()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(hb_values, vec!["Heartbeat"]);
}

#[derive(Serialize, Deserialize, TypewaySchema)]
#[serde(tag = "kind")]
enum Shape {
    Circle { radius: f64 },
    Square { side: f64 },
}

#[test]
fn internally_tagged_enum_emits_discriminator() {
    let schema = Shape::schema();
    let disc = schema
        .discriminator
        .as_ref()
        .expect("discriminator present");
    assert_eq!(disc.property_name, "kind");

    let variants = schema.one_of.as_ref().unwrap();
    assert_eq!(variants.len(), 2);

    let circle_props = variants[0].properties.as_ref().unwrap();
    assert!(circle_props.contains_key("kind"));
    assert!(circle_props.contains_key("radius"));

    let kind_schema = &circle_props["kind"];
    assert_eq!(kind_schema.schema_type.as_deref(), Some("string"));
    let kind_values: Vec<&str> = kind_schema
        .enum_values
        .as_ref()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(kind_values, vec!["Circle"]);
}

#[derive(Serialize, Deserialize, TypewaySchema)]
#[serde(tag = "type", content = "data")]
enum Message {
    Text(String),
    Ping,
}

#[test]
fn adjacently_tagged_enum_wraps_payload() {
    let schema = Message::schema();
    let disc = schema.discriminator.as_ref().expect("discriminator");
    assert_eq!(disc.property_name, "type");

    let variants = schema.one_of.as_ref().unwrap();
    let text_props = variants[0].properties.as_ref().unwrap();
    assert!(text_props.contains_key("type"));
    assert!(text_props.contains_key("data"));
    assert_eq!(text_props["data"].schema_type.as_deref(), Some("string"));

    let ping_props = variants[1].properties.as_ref().unwrap();
    assert!(ping_props.contains_key("type"));
    assert!(!ping_props.contains_key("data"));
}

#[derive(Serialize, Deserialize, TypewaySchema)]
#[serde(untagged)]
enum Either {
    Number(i64),
    Text(String),
}

#[test]
fn untagged_enum_emits_bare_payload_one_of() {
    let schema = Either::schema();
    assert!(schema.discriminator.is_none());
    let variants = schema.one_of.as_ref().unwrap();
    assert_eq!(variants[0].schema_type.as_deref(), Some("integer"));
    assert_eq!(variants[1].schema_type.as_deref(), Some("string"));
}

#[test]
fn one_of_serializes_as_openapi_one_of() {
    let schema = Event::schema();
    let json = serde_json::to_value(&schema).unwrap();
    assert!(
        json.get("oneOf").is_some(),
        "expected `oneOf` key in serialized schema"
    );
}

#[test]
fn string_enum_serializes_with_enum_key() {
    let schema = Status::schema();
    let json = serde_json::to_value(&schema).unwrap();
    assert_eq!(json["type"], "string");
    let arr = json["enum"].as_array().expect("enum array");
    assert_eq!(arr.len(), 3);
}

#[test]
fn enum_schema_round_trips_through_openapi3_parser() {
    use typeway_openapi::openapi3_to_typeway;
    use typeway_openapi::spec::*;

    let mut spec = OpenApiSpec::new("test", "1.0");
    let mut content = indexmap::IndexMap::new();
    content.insert(
        "application/json".to_string(),
        MediaType {
            schema: Some(Event::schema()),
            example: None,
        },
    );
    let mut op = Operation::new();
    op.responses.insert(
        "200".to_string(),
        Response {
            description: "ok".to_string(),
            content,
        },
    );
    let path = PathItem {
        get: Some(op),
        ..Default::default()
    };
    spec.paths.insert("/events".to_string(), path);

    // Serialize and parse back through the codegen path. The parser should
    // accept the oneOf shape without error.
    let json = serde_json::to_string(&spec).unwrap();
    let result = openapi3_to_typeway(&json);
    assert!(
        result.is_ok(),
        "openapi3_to_typeway rejected oneOf schema: {:?}",
        result
    );
}

#[test]
fn swagger2_downgrades_one_of_to_x_one_of() {
    use typeway_openapi::spec::*;
    use typeway_openapi::to_swagger2_json;

    let mut spec = OpenApiSpec::new("test", "1.0");
    let mut op = Operation::new();
    let mut content = indexmap::IndexMap::new();
    content.insert(
        "application/json".to_string(),
        MediaType {
            schema: Some(Event::schema()),
            example: None,
        },
    );
    op.responses.insert(
        "200".to_string(),
        Response {
            description: "ok".to_string(),
            content,
        },
    );
    let path = PathItem {
        get: Some(op),
        ..Default::default()
    };
    spec.paths.insert("/events".to_string(), path);

    let swagger = to_swagger2_json(&spec);
    let parsed: serde_json::Value = serde_json::from_str(&swagger).unwrap();
    let schema = &parsed["paths"]["/events"]["get"]["responses"]["200"]["schema"];
    assert!(
        schema.get("oneOf").is_none(),
        "Swagger 2.0 must not contain oneOf"
    );
    assert!(
        schema.get("x-oneOf").is_some(),
        "expected x-oneOf vendor extension"
    );
}
