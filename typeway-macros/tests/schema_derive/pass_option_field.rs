use typeway_macros::TypewaySchema;

/// A profile with optional fields.
#[derive(TypewaySchema)]
struct Profile {
    /// Display name.
    name: String,
    /// Short biography.
    bio: Option<String>,
    /// Age (optional).
    age: Option<u32>,
}

fn main() {
    let schema = <Profile as typeway_openapi::ToSchema>::schema();
    assert_eq!(schema.description.as_deref(), Some("A profile with optional fields."));
    let props = schema.properties.as_ref().unwrap();
    assert_eq!(props.len(), 3);
    assert_eq!(props["bio"].description.as_deref(), Some("Short biography."));
    assert_eq!(props["age"].description.as_deref(), Some("Age (optional)."));
}
