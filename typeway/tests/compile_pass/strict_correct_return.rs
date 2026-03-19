// Strict<E> compiles when the handler returns the correct type.

use typeway::prelude::*;
use typeway_server::typed_response::Strict;
use typeway_server::bind_strict;

typeway_path!(type TagsPath = "tags");

type API = (Strict<GetEndpoint<TagsPath, Json<Vec<String>>>>,);

// Handler returns Json<Vec<String>> which matches Res.
async fn get_tags() -> Json<Vec<String>> {
    Json(vec![])
}

fn main() {
    let _ = Server::<API>::new((bind_strict!(get_tags),));
}
