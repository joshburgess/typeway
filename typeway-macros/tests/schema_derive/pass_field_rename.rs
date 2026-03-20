use serde::{Deserialize, Serialize};
use typeway_macros::TypewaySchema;

#[derive(Serialize, Deserialize, TypewaySchema)]
#[serde(rename_all = "camelCase")]
struct Config {
    /// The base URL.
    base_url: String,
    /// Override: use the exact name "api_key".
    #[serde(rename = "api_key")]
    api_key: String,
}

fn main() {
    let schema = <Config as typeway_openapi::ToSchema>::schema();
    let props = schema.properties.as_ref().unwrap();
    // base_url should be camelCased.
    assert!(props.contains_key("baseUrl"));
    // api_key has an explicit rename, overriding rename_all.
    assert!(props.contains_key("api_key"));
    assert!(!props.contains_key("apiKey"));
}
