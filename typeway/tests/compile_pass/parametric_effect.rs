// Parametric (generic) effect types work with EffectfulServer.

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_server::effects::EffectfulServer;
use std::marker::PhantomData;

// A generic effect parameterized by a type.
struct CacheRequired<T>(PhantomData<T>);
impl<T: Send + Sync + 'static> Effect for CacheRequired<T> {}

struct UserCache;
struct ItemCache;

typeway_path!(type UsersPath = "users");
typeway_path!(type ItemsPath = "items");

type API = (
    Requires<CacheRequired<UserCache>, GetEndpoint<UsersPath, String>>,
    Requires<CacheRequired<ItemCache>, GetEndpoint<ItemsPath, String>>,
);

async fn get_users() -> &'static str { "users" }
async fn get_items() -> &'static str { "items" }

fn main() {
    // Each parameterized effect must be provided separately.
    let _server = EffectfulServer::<API>::new((bind!(get_users), bind!(get_items)))
        .provide::<CacheRequired<UserCache>>()
        .provide::<CacheRequired<ItemCache>>()
        .ready();
}
