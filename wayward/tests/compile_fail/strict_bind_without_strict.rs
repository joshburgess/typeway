// Error: using bind!() on a Strict endpoint should fail.
// Must use bind_strict!() instead.

use wayward::prelude::*;
use wayward_server::typed_response::Strict;

wayward_path!(type TagsPath = "tags");

type API = (Strict<GetEndpoint<TagsPath, String>>,);

async fn get_tags() -> String {
    "tags".into()
}

fn main() {
    let _ = Server::<API>::new((bind!(get_tags),));
}
