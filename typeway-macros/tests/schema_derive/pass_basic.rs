use typeway_macros::TypewaySchema;

/// A user account.
#[derive(TypewaySchema)]
struct User {
    /// The unique identifier.
    id: u32,
    /// Display name.
    name: String,
}

fn main() {
    let schema = <User as typeway_openapi::ToSchema>::schema();
    assert_eq!(schema.schema_type.as_deref(), Some("object"));
    assert_eq!(schema.description.as_deref(), Some("A user account."));
    let props = schema.properties.as_ref().unwrap();
    assert!(props.contains_key("id"));
    assert!(props.contains_key("name"));
    assert_eq!(
        props["id"].description.as_deref(),
        Some("The unique identifier.")
    );
    assert_eq!(
        props["name"].description.as_deref(),
        Some("Display name.")
    );

    // Verify type_name
    assert_eq!(<User as typeway_openapi::ToSchema>::type_name(), "User");
}
