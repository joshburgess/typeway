// Error: Strict<E> with handler returning wrong type.
// API declares Json<Vec<String>> but handler returns &str.

use wayward::prelude::*;
use wayward_server::typed_response::Strict;

wayward_path!(type TagsPath = "tags");

type API = (Strict<GetEndpoint<TagsPath, Json<Vec<String>>>>,);

// Wrong return type: &str instead of Json<Vec<String>>
async fn get_tags() -> &'static str {
    "wrong type"
}

fn main() {
    let _ = Server::<API>::new((bind_strict!(get_tags),));
}
