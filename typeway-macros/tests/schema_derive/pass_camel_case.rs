use serde::{Deserialize, Serialize};
use typeway_macros::TypewaySchema;

#[derive(Serialize, Deserialize, TypewaySchema)]
#[serde(rename_all = "camelCase")]
struct Article {
    /// Article title.
    article_title: String,
    /// Tag list.
    tag_list: Vec<String>,
}

fn main() {
    let schema = <Article as typeway_openapi::ToSchema>::schema();
    let props = schema.properties.as_ref().unwrap();
    assert!(props.contains_key("articleTitle"));
    assert!(props.contains_key("tagList"));
    assert!(!props.contains_key("article_title"));
    assert!(!props.contains_key("tag_list"));
    assert_eq!(
        props["articleTitle"].description.as_deref(),
        Some("Article title.")
    );
    assert_eq!(
        props["tagList"].description.as_deref(),
        Some("Tag list.")
    );
}
