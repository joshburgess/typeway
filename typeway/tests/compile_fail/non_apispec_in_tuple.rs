// Error: non-ApiSpec type used in an API tuple position.

use typeway::prelude::*;

typeway_path!(type UsersPath = "users");

// String doesn't implement ApiSpec — cannot be in an API tuple.
type API = (
    GetEndpoint<UsersPath, String>,
    String,
);

fn _check() {
    fn _assert_api<T: typeway_core::ApiSpec>() {}
    _assert_api::<API>();
}

fn main() {}
