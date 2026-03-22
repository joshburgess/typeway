// HasEndpoint trait verifies endpoint membership in API tuples.

use typeway::prelude::*;
use typeway_core::versioning::{HasEndpoint, Here, There};

typeway_path!(type UsersPath = "users");
typeway_path!(type ItemsPath = "items");
typeway_path!(type TagsPath = "tags");

type UserEndpoint = GetEndpoint<UsersPath, String>;
type ItemEndpoint = GetEndpoint<ItemsPath, String>;
type TagEndpoint = GetEndpoint<TagsPath, String>;

type API = (UserEndpoint, ItemEndpoint, TagEndpoint);

fn _check() {
    fn _assert_has<T: HasEndpoint<E, Idx>, E, Idx>() {}

    // Each endpoint is found at its position.
    _assert_has::<API, UserEndpoint, Here>();
    _assert_has::<API, ItemEndpoint, There<Here>>();
    _assert_has::<API, TagEndpoint, There<There<Here>>>();

    // Inferred index also works.
    _assert_has::<API, UserEndpoint, _>();
    _assert_has::<API, ItemEndpoint, _>();
    _assert_has::<API, TagEndpoint, _>();
}

fn main() {}
