// Error: HasEndpoint fails when the endpoint is not in the tuple.

use typeway::prelude::*;
use typeway_core::versioning::HasEndpoint;

typeway_path!(type UsersPath = "users");
typeway_path!(type ItemsPath = "items");
typeway_path!(type MissingPath = "missing");

type API = (
    GetEndpoint<UsersPath, String>,
    GetEndpoint<ItemsPath, String>,
);

// MissingEndpoint is NOT in API.
type MissingEndpoint = GetEndpoint<MissingPath, String>;

fn _check() {
    fn _assert_has<T: HasEndpoint<E, Idx>, E, Idx>() {}
    // Should fail — MissingEndpoint not in API.
    _assert_has::<API, MissingEndpoint, _>();
}

fn main() {}
