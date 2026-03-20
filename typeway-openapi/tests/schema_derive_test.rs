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
