// Error: empty string literal in typeway_path! is rejected.

use typeway_macros::typeway_path;

typeway_path!(type BadPath = "");

fn main() {}
