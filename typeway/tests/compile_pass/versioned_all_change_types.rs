// All change marker types (Added, Removed, Replaced, Deprecated) compile
// and produce correct ApiChangelog counts.

use typeway::prelude::*;
use typeway_core::versioning::*;

typeway_path!(type UsersPath = "users");
typeway_path!(type ItemsPath = "items");
typeway_path!(type TagsPath = "tags");
typeway_path!(type NotesPath = "notes");
typeway_path!(type FeedPath = "feed");

// All-Added changelog.
type AllAdded = (
    Added<GetEndpoint<UsersPath, String>>,
    Added<GetEndpoint<ItemsPath, String>>,
);
const _: () = assert!(<AllAdded as ApiChangelog>::ADDED == 2);
const _: () = assert!(<AllAdded as ApiChangelog>::REMOVED == 0);

// All-Removed changelog.
type AllRemoved = (
    Removed<GetEndpoint<TagsPath, String>>,
    Removed<GetEndpoint<NotesPath, String>>,
);
const _: () = assert!(<AllRemoved as ApiChangelog>::REMOVED == 2);
const _: () = assert!(<AllRemoved as ApiChangelog>::ADDED == 0);

// Mixed changelog.
type Mixed = (
    Added<GetEndpoint<FeedPath, String>>,
    Removed<GetEndpoint<NotesPath, String>>,
    Replaced<GetEndpoint<UsersPath, String>, GetEndpoint<UsersPath, Vec<String>>>,
    Deprecated<GetEndpoint<TagsPath, String>>,
);
const _: () = assert!(<Mixed as ApiChangelog>::ADDED == 1);
const _: () = assert!(<Mixed as ApiChangelog>::REMOVED == 1);
const _: () = assert!(<Mixed as ApiChangelog>::REPLACED == 1);
const _: () = assert!(<Mixed as ApiChangelog>::DEPRECATED == 1);

// Single-element changelog.
type SingleAdded = (Added<GetEndpoint<UsersPath, String>>,);
const _: () = assert!(<SingleAdded as ApiChangelog>::ADDED == 1);

fn main() {}
