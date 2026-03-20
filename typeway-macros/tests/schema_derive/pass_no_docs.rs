use typeway_macros::TypewaySchema;

#[derive(TypewaySchema)]
struct Minimal {
    x: u32,
    y: String,
}

fn main() {
    let schema = <Minimal as typeway_openapi::ToSchema>::schema();
    assert_eq!(schema.schema_type.as_deref(), Some("object"));
    // No struct-level doc comment.
    assert!(schema.description.is_none());
    let props = schema.properties.as_ref().unwrap();
    assert!(props.contains_key("x"));
    assert!(props.contains_key("y"));
    // No field-level doc comments.
    assert!(props["x"].description.is_none());
    assert!(props["y"].description.is_none());
}
