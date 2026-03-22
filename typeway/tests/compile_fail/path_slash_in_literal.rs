// Error: slash inside a path literal is rejected — use separate segments.

use typeway_macros::typeway_path;

typeway_path!(type BadPath = "users/posts");

fn main() {}
